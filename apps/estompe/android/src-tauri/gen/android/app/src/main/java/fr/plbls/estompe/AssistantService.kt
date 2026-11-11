package fr.plbls.estompe

import android.Manifest
import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.media.AudioManager
import android.media.ToneGenerator
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import kotlin.concurrent.thread

sealed class ReplyTo {
  data class Text(val number: String, val name: String) : ReplyTo()
  data class Note(val key: String, val name: String, val label: String, val replyable: Boolean) : ReplyTo()
}

data class Unread(val heard: Heard, val time: Long, val to: ReplyTo)

enum class Confirm { SEND, RETRY, CANCEL, ASK }

const val NOT_UNDERSTOOD = "Je n'ai pas compris. Dites par exemple : lis mes messages, ou mets Daft Punk."

private val CHOICE_WORDS = listOf("un", "deux", "trois", "quatre")

fun scopeOf(name: Any?): AssistantScope? = (name as? String)?.let { n -> AssistantScope.values().firstOrNull { it.name.equals(n.trim(), ignoreCase = true) } }

fun settingsFrom(j: JSONObject, s: AssistantSettings): AssistantSettings {
  val apps = (j.opt("apps") as? JSONArray)?.let { a -> (0 until a.length()).mapNotNull { (a.opt(it) as? String)?.trim()?.takeIf { p -> p.isNotEmpty() } }.toSet() }
  return AssistantSettings(scope = scopeOf(j.opt("scope")) ?: s.scope, apps = apps ?: s.apps,
    musicApp = (j.opt("musicApp") as? String)?.trim() ?: s.musicApp, llm = j.opt("llm") as? Boolean ?: s.llm)
}

fun settingsJson(s: AssistantSettings): JSONObject = JSONObject().put("scope", s.scope.name.lowercase())
  .put("apps", JSONArray(s.apps.sorted())).put("musicApp", s.musicApp).put("llm", s.llm)

fun shouldRun(s: AssistantSettings, driving: Boolean) = s.scope == AssistantScope.ALWAYS || (s.scope == AssistantScope.DRIVE && driving)

fun mergeUnread(sms: List<Sms>, notes: List<Pending>, spoken: Set<String>): List<Unread> =
  (sms.map { Unread(Heard("", it.sender, it.body), it.date, ReplyTo.Text(it.address, it.sender)) } +
    notes.filter { it.key !in spoken }.map { Unread(Heard(it.label, it.sender, it.text), it.time, ReplyTo.Note(it.key, it.sender, it.label, it.replyable)) })
    .sortedBy { it.time }

fun choices(intro: String, options: List<String>): String =
  "$intro : " + options.take(CHOICE_WORDS.size).mapIndexed { i, o -> "${CHOICE_WORDS[i]}, $o" }.joinToString(" ; ") + ". Lequel ?"

fun confirmStep(answer: Answer, asked: Int): Confirm = when (answer) {
  Answer.SEND -> Confirm.SEND
  Answer.RETRY -> Confirm.RETRY
  Answer.CANCEL -> Confirm.CANCEL
  Answer.UNKNOWN -> if (asked < 2) Confirm.ASK else Confirm.CANCEL
}

fun review(to: ReplyTo, text: String): String = when (to) {
  is ReplyTo.Text -> "SMS pour ${to.name} : $text. Envoyer, annuler ou recommencer ?"
  is ReplyTo.Note -> "Réponse à ${to.name} sur ${to.label} : $text. Envoyer, annuler ou recommencer ?"
}

fun musicFailure(listener: Boolean): String =
  if (!listener) "Je n'ai rien à piloter. Accordez l'accès aux notifications dans Estompe." else "L'application de musique n'a pas répondu."

class AssistantStore(context: Context) {
  private val prefs = context.applicationContext.getSharedPreferences("assistant", Context.MODE_PRIVATE)

  fun load() = AssistantSettings(
    scope = scopeOf(prefs.getString("scope", "")) ?: AssistantScope.OFF,
    apps = prefs.getStringSet("apps", null).orEmpty().toSet(),
    musicApp = prefs.getString("musicApp", "").orEmpty(),
    llm = prefs.getBoolean("llm", false),
  )

  fun toJson(s: AssistantSettings = load()): JSONObject = settingsJson(s)

  fun save(j: JSONObject) {
    val s = settingsFrom(j, load())
    prefs.edit().putString("scope", s.scope.name.lowercase()).putStringSet("apps", HashSet(s.apps)).putString("musicApp", s.musicApp)
      .putBoolean("llm", s.llm).apply()
  }
}

class AssistantService : Service() {
  private val ui = Handler(Looper.getMainLooper())
  private lateinit var voice: Voice
  private lateinit var phone: Phone
  private lateinit var music: MusicControl
  private var settings = AssistantSettings()
  private var apps: List<MusicApp> = emptyList()
  private var started = false
  private var destroyed = false
  private var busy = false
  private var busySince = 0L
  private var notice: String? = null
  private var failures = 0
  private var last: ReplyTo? = null
  private val spoken = LinkedHashSet<String>()
  private var llm: Llm? = null
  private var tone: ToneGenerator? = null
  private val resumeLater = Runnable { resume() }
  private val unload = Runnable { closeLlm() }
  private val tick = object : Runnable {
    override fun run() {
      check()
      ui.postDelayed(this, TICK_MS)
    }
  }
  private val freed: () -> Unit = {
    if (!destroyed) { ui.removeCallbacks(resumeLater); ui.postDelayed(resumeLater, RESUME_MS) }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    alive = true
    ensureChannels(this)
    phone = Phone(this)
    music = MusicControl(this)
    voice = Voice(this, DriveStore(this).load(), {}, { text -> DriveService.note("assistant : $text"); notice = text; status(text) })
    Mic.freed = freed
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      listening = false
      stopSelf()
      return START_NOT_STICKY
    }
    if (started) foreground(true) else begin()
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    destroyed = true
    alive = false
    listening = false
    if (Mic.freed === freed) Mic.freed = null
    ui.removeCallbacksAndMessages(null)
    voice.shutdown()
    closeLlm()
    releaseTone()
    super.onDestroy()
  }

  private fun begin() {
    val why = blocked(this)
    if (why == null && foreground(true)) {
      started = true
      cancelPause(this)
      refreshApps()
      ui.post(tick)
      return
    }
    foreground(false)
    listening = false
    pause(this, why ?: PAUSE_BACKGROUND)
    stopSelf()
  }

  private fun foreground(mic: Boolean): Boolean {
    val types = if (mic) listOf(if (Build.VERSION.SDK_INT >= 30) ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE else 0)
    else listOfNotNull(if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else null, 0)
    for (type in types) {
      try {
        ServiceCompat.startForeground(this, NOTIF_ID, notification(), type)
        return true
      } catch (e: Exception) {
        DriveService.note("assistant au premier plan : ${e.javaClass.simpleName}")
      }
    }
    return false
  }

  private fun notification(text: String? = null): Notification {
    val open = PendingIntent.getActivity(this, 10, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE)
    val stop = PendingIntent.getService(this, 11, Intent(this, AssistantService::class.java).setAction(ACTION_STOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    return NotificationCompat.Builder(this, "drive").setSmallIcon(R.drawable.ic_notification).setColor(getColor(R.color.notification_accent))
      .setContentTitle("Assistant vocal").setContentText(text ?: notice ?: "Dis “Estompe” pour donner un ordre.")
      .setOngoing(true).setOnlyAlertOnce(true).setSilent(true).setPriority(NotificationCompat.PRIORITY_LOW).setContentIntent(open)
      .addAction(0, "Arrêter", stop).build()
  }

  private fun status(text: String) {
    try { NotificationManagerCompat.from(this).notify(NOTIF_ID, notification(text)) } catch (e: SecurityException) { DriveService.note("assistant : notification refusée") }
  }

  private fun check() {
    if (busy && SystemClock.elapsedRealtime() - busySince > BUSY_MAX_MS) {
      DriveService.note("assistant : ordre bloqué, retour à la veille")
      done()
    }
    if (!VoiceService.running) return resume()
    voice.unwatch()
    listening = false
  }

  private fun resume() {
    if (destroyed || !started || busy || Mic.busy || VoiceService.running) return
    voice.watch(::heard) { on, failed ->
      listening = on
      if (failed && !destroyed && ++failures >= MAX_FAILURES) {
        pause(this, PAUSE_BUSY)
        stopSelf()
      }
    }
  }

  private fun heard(text: String) {
    failures = 0
    if (busy || destroyed) return
    val order = wakeWord(text) ?: return
    busy = true
    busySince = SystemClock.elapsedRealtime()
    voice.unwatch()
    settings = AssistantStore(this).load()
    voice.focus()
    if (order.isNotBlank()) return execute(order)
    beep()
    voice.listen(ORDER_MS, false) { t -> if (t.isNullOrBlank()) done() else execute(t) }
  }

  private fun execute(text: String) {
    val c = try { parseCommand(text, apps) } catch (e: Exception) {
      DriveService.note("assistant : analyse impossible (${e.javaClass.simpleName})")
      Command.Unknown
    }
    if (c == Command.Unknown) understand(text) else run(c)
  }

  private fun understand(text: String) {
    if (!settings.llm || !LlmModel.AVAILABLE || !LlmModel.ready(this)) return run(Command.Unknown)
    ui.removeCallbacks(unload)
    val model = llm ?: Llm(this).also { llm = it }
    var answered = false
    val wait = Runnable { if (!answered && !destroyed) voice.say("Un instant.") }
    ui.postDelayed(wait, WAIT_MS)
    model.understand(text, apps) { c ->
      if (answered || destroyed) return@understand
      answered = true
      ui.removeCallbacks(wait)
      ui.removeCallbacks(unload)
      ui.postDelayed(unload, LLM_IDLE_MS)
      run(c)
    }
  }

  private fun run(c: Command) {
    try {
      when (c) {
        Command.Unknown -> finish(NOT_UNDERSTOOD)
        Command.Help -> finish(HELP_TEXT)
        Command.Cancel -> finish("D'accord.")
        Command.ReadUnread -> readUnread()
        is Command.ReadFrom -> readFrom(c.contact)
        Command.Reply -> reply()
        is Command.SendSms -> sendSms(c.contact, c.text)
        is Command.Music -> background({ music.transport(c.action, c.app.ifEmpty { settings.musicApp }) }) { played(it == true) }
        is Command.PlaySearch -> music.search(c.query, c.app.ifEmpty { settings.musicApp }) { played(it) }
        is Command.PlayPlaylist -> music.playlist(c.name, c.app.ifEmpty { settings.musicApp }) { played(it) }
      }
    } catch (e: Exception) {
      failed(e)
    }
  }

  private fun failed(e: Exception) {
    DriveService.note("assistant : ordre interrompu (${e.javaClass.simpleName})")
    if (busy && !destroyed) finish("Une erreur est survenue.")
  }

  private fun played(ok: Boolean) {
    if (destroyed) return
    if (ok) done() else finish(musicFailure(AssistantListener.enabled(this)))
  }

  private fun readUnread() {
    val sms = phone.canReadSms()
    val notes = AssistantListener.enabled(this)
    if (!sms && !notes) return finish("Autorisez les SMS ou l'accès aux notifications dans Estompe.")
    val chosen = settings.apps
    background({ Pair(if (sms) phone.unreadSms() else emptyList(), if (notes) AssistantListener.pending(chosen) else emptyList()) }) { r ->
      val texts = r?.first.orEmpty()
      val items = mergeUnread(texts, r?.second.orEmpty(), spoken)
      items.lastOrNull()?.let { last = it.to }
      sayAll(announceMessages(items.map { it.heard })) {
        val ids = texts.map { it.id }
        if (ids.isNotEmpty()) thread { phone.markSpoken(ids) }
        items.forEach { (it.to as? ReplyTo.Note)?.let { n -> remember(n.key) } }
        done()
      }
    }
  }

  private fun readFrom(name: String) {
    if (!phone.canReadSms()) return finish("Autorisez la lecture des SMS dans Estompe.")
    contact(name) { c ->
      background({ phone.smsFrom(c) }) { list ->
        if (list.isNullOrEmpty()) return@background finish("Aucun message de ${c.name}.")
        last = ReplyTo.Text(list.last().address, c.name)
        sayAll(announceMessages(list.map { Heard("", c.name, it.body) })) { done() }
      }
    }
  }

  private fun reply() {
    when (val to = last) {
      null -> finish("Aucun message auquel répondre.")
      is ReplyTo.Text -> if (phone.canSendSms()) compose(to, "") else finish("Autorisez l'envoi de SMS dans Estompe.")
      is ReplyTo.Note -> if (to.replyable) compose(to, "") else finish("Cette application ne permet pas de répondre.")
    }
  }

  private fun sendSms(name: String, text: String) {
    if (!phone.canSendSms()) return finish("Autorisez l'envoi de SMS dans Estompe.")
    contact(name) { c -> number(c) { n -> compose(ReplyTo.Text(n, c.name), text) } }
  }

  private fun contact(name: String, then: (Contact) -> Unit) {
    if (!phone.canReadContacts()) return finish("Autorisez l'accès aux contacts dans Estompe.")
    background({ matchContacts(name, phone.contacts()) }) { found ->
      val list = found.orEmpty().take(CHOICE_WORDS.size)
      when (list.size) {
        0 -> finish("Je ne trouve pas $name dans vos contacts.")
        1 -> then(list[0])
        else -> choose("Plusieurs contacts", list.map { it.name }) { then(list[it]) }
      }
    }
  }

  private fun number(c: Contact, then: (String) -> Unit) {
    val numbers = c.numbers.filter { it.isNotBlank() }.take(CHOICE_WORDS.size)
    when (numbers.size) {
      0 -> finish("Aucun numéro pour ${c.name}.")
      1 -> then(numbers[0])
      else -> choose("Plusieurs numéros pour ${c.name}", numbers.map { Phone.speakable(it) }) { then(numbers[it]) }
    }
  }

  private fun choose(intro: String, options: List<String>, asked: Int = 1, picked: (Int) -> Unit) {
    voice.say(choices(intro, options)) {
      voice.listen(CHOICE_MS, false) { heard ->
        val t = heard.orEmpty()
        val i = pickChoice(t, options.size)
        when {
          i != null -> guard { picked(i) }
          parseCommand(t) == Command.Cancel -> finish("D'accord.")
          asked < 2 -> choose(intro, options, asked + 1, picked)
          else -> finish("Je n'ai pas compris, j'abandonne.")
        }
      }
    }
  }

  private fun compose(to: ReplyTo, text: String) {
    if (text.isBlank()) dictate(to) else confirm(to, text, 1)
  }

  private fun dictate(to: ReplyTo) {
    voice.say("Quel est le message ?") {
      voice.listen(DICTATE_MS) { t ->
        when {
          t.isNullOrBlank() -> finish("Je n'ai rien entendu.")
          parseCommand(t) == Command.Cancel -> finish("Message annulé.")
          else -> confirm(to, t, 1)
        }
      }
    }
  }

  private fun confirm(to: ReplyTo, text: String, asked: Int) {
    voice.say(review(to, text)) {
      voice.listen(CONFIRM_MS) { answer ->
        when (confirmStep(parseAnswer(answer.orEmpty()), asked)) {
          Confirm.SEND -> deliver(to, text)
          Confirm.RETRY -> dictate(to)
          Confirm.CANCEL -> finish("Message annulé.")
          Confirm.ASK -> confirm(to, text, asked + 1)
        }
      }
    }
  }

  private fun deliver(to: ReplyTo, text: String) {
    when (to) {
      is ReplyTo.Text -> phone.send(to.number, text) { ok -> if (!destroyed) finish(if (ok) "Message envoyé." else "Envoi impossible.") }
      is ReplyTo.Note -> background({ AssistantListener.reply(to.key, text) }) { ok -> finish(if (ok == true) "Message envoyé." else "Envoi impossible.") }
    }
  }

  private fun sayAll(lines: List<String>, then: () -> Unit) {
    if (lines.isEmpty()) return then()
    lines.forEachIndexed { i, line -> voice.say(line, if (i == lines.lastIndex) then else ({})) }
  }

  private fun finish(text: String) {
    voice.say(text) { done() }
  }

  private fun done() {
    if (destroyed) return
    busy = false
    voice.abandonFocus()
    releaseTone()
    refreshApps()
    ui.removeCallbacks(resumeLater)
    ui.postDelayed(resumeLater, RESUME_MS)
  }

  private fun guard(block: () -> Unit) {
    try { block() } catch (e: Exception) { failed(e) }
  }

  private fun <T> background(job: () -> T, then: (T?) -> Unit) {
    thread {
      val r = try { job() } catch (e: Exception) {
        DriveService.note("assistant : lecture impossible (${e.javaClass.simpleName})")
        null
      }
      ui.post { if (!destroyed) guard { then(r) } }
    }
  }

  private fun refreshApps() {
    thread {
      val found = music.apps()
      ui.post { if (!destroyed) apps = found }
    }
  }

  private fun remember(key: String) {
    spoken.add(key)
    while (spoken.size > SPOKEN_MAX) spoken.remove(spoken.first())
  }

  private fun beep() {
    releaseTone()
    try {
      val t = ToneGenerator(AudioManager.STREAM_MUSIC, 80)
      tone = t
      t.startTone(ToneGenerator.TONE_PROP_BEEP, 150)
    } catch (e: Exception) {
      DriveService.note("assistant : signal sonore ${e.javaClass.simpleName}")
    }
  }

  private fun releaseTone() {
    try { tone?.release() } catch (e: Exception) { DriveService.note("assistant : signal sonore ${e.javaClass.simpleName}") }
    tone = null
  }

  private fun closeLlm() {
    ui.removeCallbacks(unload)
    try { llm?.close() } catch (e: Exception) { DriveService.note("assistant : libération du modèle de langue ${e.javaClass.simpleName}") }
    llm = null
  }

  companion object {
    const val ACTION_SYNC = "fr.plbls.estompe.assistant.SYNC"
    const val ACTION_STOP = "fr.plbls.estompe.assistant.STOP"
    private const val NOTIF_ID = 4245
    private const val PAUSE_ID = 4246
    private const val TICK_MS = 3_000L
    private const val RESUME_MS = 500L
    private const val WAIT_MS = 1_500L
    private const val LLM_IDLE_MS = 120_000L
    private const val ORDER_MS = 6_000
    private const val CHOICE_MS = 6_000
    private const val DICTATE_MS = 8_000
    private const val CONFIRM_MS = 7_000
    private const val SPOKEN_MAX = 300
    private const val MAX_FAILURES = 5
    private const val BUSY_MAX_MS = 300_000L
    private const val PAUSE_BACKGROUND = "Assistant vocal en pause : touche pour relancer l'écoute."
    private const val PAUSE_MIC = "Assistant vocal en pause : autorise le micro dans Estompe."
    private const val PAUSE_MODEL = "Assistant vocal en pause : télécharge le modèle de reconnaissance dans Estompe."
    private const val PAUSE_BUSY = "Assistant vocal en pause : micro indisponible, touche pour relancer l'écoute."

    @Volatile var listening = false
    @Volatile private var alive = false

    fun sync(c: Context) {
      try {
        if (!shouldRun(AssistantStore(c).load(), DriveService.running)) return stop(c)
        if (alive) return
        blocked(c)?.let { listening = false; return pause(c, it) }
        ContextCompat.startForegroundService(c, Intent(c, AssistantService::class.java).setAction(ACTION_SYNC))
      } catch (e: Exception) {
        DriveService.note("démarrage de l'assistant vocal : ${e.javaClass.simpleName}")
        listening = false
        pause(c, PAUSE_BACKGROUND)
      }
    }

    fun stop(c: Context) {
      listening = false
      cancelPause(c)
      try { c.stopService(Intent(c, AssistantService::class.java)) } catch (e: Exception) {
        DriveService.note("arrêt de l'assistant vocal : ${e.javaClass.simpleName}")
      }
    }

    private fun blocked(c: Context): String? = when {
      ContextCompat.checkSelfPermission(c, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED -> PAUSE_MIC
      !VoskModel.ready(c) -> PAUSE_MODEL
      else -> null
    }

    private fun pause(c: Context, text: String) {
      DriveService.note(text)
      try {
        ensureChannels(c)
        val open = PendingIntent.getActivity(c, 12, Intent(c, MainActivity::class.java)
          .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP), PendingIntent.FLAG_IMMUTABLE)
        NotificationManagerCompat.from(c).notify(PAUSE_ID, NotificationCompat.Builder(c, "drive").setSmallIcon(R.drawable.ic_notification)
          .setColor(c.getColor(R.color.notification_accent)).setContentTitle("Assistant vocal").setContentText(text)
          .setOnlyAlertOnce(true).setAutoCancel(true).setContentIntent(open).build())
      } catch (e: Exception) {
        DriveService.note("assistant : notification de pause ${e.javaClass.simpleName}")
      }
    }

    private fun cancelPause(c: Context) {
      try { NotificationManagerCompat.from(c).cancel(PAUSE_ID) } catch (e: Exception) {
        DriveService.note("assistant : notification de pause ${e.javaClass.simpleName}")
      }
    }
  }
}
