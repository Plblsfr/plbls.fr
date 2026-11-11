package fr.plbls.estompe

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import org.json.JSONObject
import java.io.File
import java.util.zip.ZipInputStream
import kotlin.concurrent.thread

/** Réglages du mode conduite, gardés côté Kotlin (le service tourne sans WebView). */
class DriveStore(context: Context) {
  private val prefs = context.applicationContext.getSharedPreferences("drive", Context.MODE_PRIVATE)

  fun load() = DriveSettings(
    engine = prefs.getString("engine", "").orEmpty(),
    rate = prefs.getFloat("rate", 1f),
    readContent = prefs.getBoolean("readContent", true),
    confirm = prefs.getBoolean("confirm", true),
    autoBluetooth = prefs.getBoolean("autoBluetooth", false),
    bluetooth = prefs.getString("bluetooth", "").orEmpty(),
  )

  fun toJson(s: DriveSettings = load()) = JSONObject().put("engine", s.engine).put("rate", s.rate.toDouble())
    .put("readContent", s.readContent).put("confirm", s.confirm).put("autoBluetooth", s.autoBluetooth).put("bluetooth", s.bluetooth)

  /** Les champs absents ou mal typés gardent leur valeur actuelle. */
  fun save(j: JSONObject) {
    val s = load()
    prefs.edit()
      .putString("engine", j.optString("engine", s.engine))
      .putFloat("rate", j.optDouble("rate", s.rate.toDouble()).toFloat().coerceIn(0.5f, 2f))
      .putBoolean("readContent", j.optBoolean("readContent", s.readContent))
      .putBoolean("confirm", j.optBoolean("confirm", s.confirm))
      .putBoolean("autoBluetooth", j.optBoolean("autoBluetooth", s.autoBluetooth))
      .putString("bluetooth", j.optString("bluetooth", s.bluetooth))
      .apply()
  }
}

/** Modèle de reconnaissance vocale Vosk (français, ~40 Mo), téléchargé une fois ; la reconnaissance reste sur l'appareil. */
object VoskModel {
  private const val NAME = "vosk-model-small-fr-0.22"
  private const val URL = "https://alphacephei.com/vosk/models/$NAME.zip"

  /** « busy » pendant le téléchargement, sinon le dernier message d'erreur (vide si tout va bien). */
  @Volatile var state = ""

  private fun dir(c: Context) = File(c.filesDir, NAME)
  fun ready(c: Context) = File(dir(c), ".ok").exists()
  fun path(c: Context): String = dir(c).path

  fun download(c: Context) {
    if (state == "busy" || ready(c)) return
    state = "busy"
    val ctx = c.applicationContext
    thread {
      val tmp = File(ctx.filesDir, "$NAME.tmp")
      try {
        tmp.deleteRecursively()
        java.net.URL(URL).openStream().use { raw ->
          ZipInputStream(raw).use { zip ->
            generateSequence { zip.nextEntry }.forEach { e ->
              val out = File(tmp, e.name)
              // l'archive vient d'Internet : pas de chemin hors du dossier cible
              if (!out.canonicalPath.startsWith(tmp.canonicalPath + File.separator)) throw SecurityException("archive invalide")
              if (e.isDirectory) out.mkdirs() else { out.parentFile?.mkdirs(); out.outputStream().use { zip.copyTo(it) } }
            }
          }
        }
        val inner = File(tmp, NAME)
        dir(ctx).deleteRecursively()
        if (!inner.renameTo(dir(ctx))) throw IllegalStateException("modèle introuvable dans l'archive")
        File(dir(ctx), ".ok").createNewFile()
        state = ""
      } catch (e: Exception) {
        DriveService.note("téléchargement du modèle : $e")
        state = "téléchargement impossible : ${e.message}"
      } finally {
        tmp.deleteRecursively()
      }
    }
  }
}

/**
 * Mode conduite : lit à voix haute les messages privés et mentions (synthèse du téléphone), puis permet d'y répondre
 * en dictant (Vosk embarqué). Aucun service Google, aucune donnée vocale hors de l'appareil.
 */
class DriveService : Service() {
  private val ui = Handler(Looper.getMainLooper())
  private val queue = ReadQueue()
  private lateinit var settings: DriveSettings
  private lateinit var voice: Voice
  private var session: MediaSession? = null
  private var busy = false
  private var destroyed = false
  private var last: Incoming? = null
  private val pumpLater: () -> Unit = { if (!destroyed) pump() }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    running = true
    settings = DriveStore(this).load()
    ensureChannels(this)
    voice = Voice(this, settings, { pump() }, ::noEngine)
    AssistantService.sync(this)
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    when (intent?.action) {
      ACTION_STOP -> { stopSelf(); return START_NOT_STICKY }
      ACTION_REPLY -> { foreground(false); reply() }
      ACTION_MESSAGE -> {
        foreground(false)
        val d = intent.extras?.keySet().orEmpty().mapNotNull { k -> intent.getStringExtra(k)?.let { k to it } }.toMap()
        Incoming.from(d)?.takeIf { it.kind in Incoming.SPOKEN_KINDS && queue.offer(it) }?.let { pump() }
      }
      else -> foreground(false)
    }
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    running = false
    destroyed = true
    Mic.forget(pumpLater)
    ui.removeCallbacksAndMessages(null)
    queue.clear()
    voice.shutdown()
    session?.release()
    AssistantService.sync(this)
    super.onDestroy()
  }

  private fun foreground(mic: Boolean) {
    var type = 0
    if (Build.VERSION.SDK_INT >= 29) type = ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK
    if (mic && Build.VERSION.SDK_INT >= 30) type = type or ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    try {
      ServiceCompat.startForeground(this, NOTIF_ID, notification(), type)
    } catch (e: Exception) {
      // démarrage en arrière-plan refusé (Android 12+) ou micro non autorisé : on poursuit sans, la dictée échouera proprement
      note("service au premier plan : $e")
    }
  }

  private fun notification(text: String? = null): Notification {
    fun action(name: String, requestCode: Int) = PendingIntent.getService(this, requestCode,
      Intent(this, DriveService::class.java).setAction(name), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE)
    return NotificationCompat.Builder(this, "drive").setSmallIcon(R.drawable.ic_notification).setColor(getColor(R.color.notification_accent))
      .setContentTitle("Mode conduite").setContentText(text ?: "Les messages privés et mentions sont lus à voix haute.")
      .setOngoing(true).setOnlyAlertOnce(true).setContentIntent(open)
      .addAction(0, "Répondre", action(ACTION_REPLY, 1))
      .addAction(0, "Arrêter", action(ACTION_STOP, 2)).build()
  }

  private fun status(text: String) {
    try { NotificationManagerCompat.from(this).notify(NOTIF_ID, notification(text)) } catch (e: SecurityException) { /* permission retirée */ }
  }

  private fun noEngine(text: String) {
    note(text)
    val install = PendingIntent.getActivity(this, 3, Intent(TextToSpeech.Engine.ACTION_INSTALL_TTS_DATA).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
      PendingIntent.FLAG_IMMUTABLE)
    try {
      NotificationManagerCompat.from(this).notify(NOTIF_ID, NotificationCompat.Builder(this, "drive").setSmallIcon(R.drawable.ic_notification).setColor(getColor(R.color.notification_accent))
        .setContentTitle("Mode conduite").setContentText(text).setOngoing(true).setContentIntent(install)
        .addAction(0, "Arrêter", PendingIntent.getService(this, 2, Intent(this, DriveService::class.java).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE)).build())
    } catch (e: SecurityException) { /* permission retirée */ }
  }

  /** Lit les messages en attente, un à la fois ; le dernier lu devient la cible de « Répondre ». */
  private fun pump() {
    if (busy || !voice.isReady) return
    if (!Mic.quiet) return Mic.whenQuiet(pumpLater)
    val m = queue.poll() ?: return voice.abandonFocus()
    busy = true
    last = m
    status(if (m.hidden) m.title else "Dernier message : ${m.sender}")
    voice.say(announce(m, settings.readContent)) { busy = false; armReply(m); pump() }
  }

  /** Quelques secondes après une lecture, le bouton lecture/pause du volant lance la réponse (sans voler les touches de la musique le reste du temps). */
  private fun armReply(m: Incoming) {
    if (!m.replyable || queue.size > 0) return
    val s = session ?: MediaSession(this, "estompe-drive").also { s ->
      s.setCallback(object : MediaSession.Callback() {
        override fun onPlay() = reply()
        override fun onPause() = reply()
        override fun onSkipToNext() = reply()
      })
      session = s
    }
    s.setPlaybackState(PlaybackState.Builder().setState(PlaybackState.STATE_PLAYING, 0, 1f)
      .setActions(PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_SKIP_TO_NEXT).build())
    s.isActive = true
    ui.removeCallbacks(disarm)
    ui.postDelayed(disarm, ARM_MS)
  }

  private val disarm = Runnable { session?.isActive = false }

  private fun reply() {
    val m = last
    if (busy || !Mic.quiet) return
    if (m == null || !m.replyable) return voice.say("Aucun message à répondre.")
    if (!VoskModel.ready(this)) return voice.say("Modèle de reconnaissance absent. Ouvre Estompe pour le télécharger.")
    busy = true
    foreground(true) // le micro exige le type « microphone », accordé ici car l'utilisateur vient d'agir
    dictate(m)
  }

  private fun dictate(m: Incoming) {
    voice.say("Je vous écoute.") {
      voice.listen(8000, exclusive = true) { heard ->
        if (heard.isNullOrBlank()) return@listen finish("Je n'ai rien entendu.")
        val ask = if (settings.confirm) "Envoyer, annuler ou recommencer ?" else "Envoi dans quelques secondes. Dites annuler pour stopper."
        voice.say("Vous avez dit : $heard. $ask") { said ->
          if (!said) return@say finish("Réponse annulée.")
          voice.listen(if (settings.confirm) 7000 else 3500, exclusive = true) { answer ->
            when (parseAnswer(answer.orEmpty())) {
              Answer.SEND -> send(m, heard)
              Answer.CANCEL -> finish("Réponse annulée.")
              Answer.RETRY -> dictate(m)
              Answer.UNKNOWN -> if (settings.confirm) finish("Réponse annulée.") else send(m, heard)
            }
          }
        }
      }
    }
  }

  private fun send(m: Incoming, text: String) {
    thread {
      val code = Session(this).call("POST", convPath(m.conv) + "/messages", JSONObject().put("content", text).put("reply_to", m.msg))
      if (code !in 200..299) note("envoi de la réponse : code $code")
      ui.post { if (!destroyed) finish(if (code in 200..299) "Message envoyé." else "Envoi impossible.") }
    }
  }

  private fun finish(text: String) {
    voice.say(text) { busy = false; foreground(false); pump() }
  }

  companion object {
    const val ACTION_START = "fr.plbls.estompe.drive.START"
    const val ACTION_STOP = "fr.plbls.estompe.drive.STOP"
    const val ACTION_REPLY = "fr.plbls.estompe.drive.REPLY"
    const val ACTION_MESSAGE = "fr.plbls.estompe.drive.MESSAGE"
    private const val NOTIF_ID = 4242
    private const val ARM_MS = 20_000L

    @Volatile var running = false
    private val journal = ArrayDeque<String>()

    /** Erreurs de synthèse et de reconnaissance, relues par le mode debug (bridge `driveLog`). */
    @Synchronized fun note(line: String) {
      Log.w("EstompeDrive", line)
      journal.addLast(line)
      if (journal.size > 50) journal.removeFirst()
    }

    @Synchronized fun log(): String = journal.joinToString("\n")

    fun start(c: Context) = try {
      androidx.core.content.ContextCompat.startForegroundService(c, Intent(c, DriveService::class.java).setAction(ACTION_START))
    } catch (e: Exception) {
      note("démarrage du mode conduite : $e") // Android 12+ refuse un démarrage en arrière-plan hors exemption
    }

    fun stop(c: Context) { c.stopService(Intent(c, DriveService::class.java)) }

    /** Message FCM reçu : lu seulement si le mode conduite tourne. */
    fun read(c: Context, data: Map<String, String>) {
      if (!running || data["kind"] !in Incoming.SPOKEN_KINDS) return
      c.startService(Intent(c, DriveService::class.java).setAction(ACTION_MESSAGE).apply { data.forEach { (k, v) -> putExtra(k, v) } })
    }
  }
}
