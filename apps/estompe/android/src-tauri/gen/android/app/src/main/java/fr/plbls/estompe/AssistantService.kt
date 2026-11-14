package fr.plbls.estompe

import android.Manifest
import android.app.Notification
import android.app.PendingIntent
import android.app.role.RoleManager
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
import android.os.PowerManager
import android.os.SystemClock
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.concurrent.thread

sealed class ReplyTo {
  data class Text(val number: String, val name: String, val several: Boolean = false) : ReplyTo()
  data class Note(val key: String, val name: String, val label: String, val replyable: Boolean) : ReplyTo()
}

data class Unread(val heard: Heard, val time: Long, val to: ReplyTo, val sms: Long? = null, val notes: List<String> = emptyList())

enum class Confirm { SEND, RETRY, CANCEL, ASK }

const val NOT_UNDERSTOOD = "Je n'ai pas compris. Dites par exemple : lis mes messages, ou mets Daft Punk."

const val REPEAT = "Je n'ai pas compris. Répétez ?"

const val PROMPT = "Oui ?"

const val LISTENING = "J'écoute…"

const val SPEAKING = "Je réponds…"

const val NOTES_MISSING = "Les notifications ne sont pas encore accessibles."

const val READ_MAX = 5

private val CHOICE_WORDS = listOf("un", "deux", "trois", "quatre")

val CHOICE_MAX = CHOICE_WORDS.size

fun scopeOf(name: Any?): AssistantScope? = (name as? String)?.let { n -> AssistantScope.values().firstOrNull { it.name.equals(n.trim(), ignoreCase = true) } }

fun settingsFrom(j: JSONObject, s: AssistantSettings): AssistantSettings {
  val apps = (j.opt("apps") as? JSONArray)?.let { a -> (0 until a.length()).mapNotNull { (a.opt(it) as? String)?.trim()?.takeIf { p -> p.isNotEmpty() } }.toSet() }
  return AssistantSettings(scope = scopeOf(j.opt("scope")) ?: s.scope, apps = apps ?: s.apps,
    musicApp = (j.opt("musicApp") as? String)?.trim() ?: s.musicApp, llm = j.opt("llm") as? Boolean ?: s.llm)
}

fun settingsJson(s: AssistantSettings): JSONObject = JSONObject().put("scope", s.scope.name.lowercase())
  .put("apps", JSONArray(s.apps.sorted())).put("musicApp", s.musicApp).put("llm", s.llm)

fun shouldRun(s: AssistantSettings, driving: Boolean) = s.scope == AssistantScope.ALWAYS || (s.scope == AssistantScope.DRIVE && driving)

fun mergeUnread(sms: List<Sms>, notes: List<Pending>, spoken: Set<String>): List<Unread> {
  val fresh = notes.filter { it.key !in spoken }
  fun same(s: Sms, p: Pending) = fold(p.text) == fold(s.body) && (fold(p.sender) == fold(s.sender) || samePhone(p.sender, s.address))
  val twins = fresh.filter { p -> sms.any { same(it, p) } }.map { it.key }.toSet()
  return (sms.map { s -> Unread(Heard("", s.sender, s.body, s.date), s.date, ReplyTo.Text(s.address, s.sender), s.id, fresh.filter { same(s, it) }.map { it.key }) } +
    fresh.filter { it.key !in twins }.map { Unread(Heard(it.label, it.sender, it.text, it.time), it.time, ReplyTo.Note(it.key, it.sender, it.label, it.replyable)) })
    .sortedBy { it.time }
}

fun choices(intro: String, options: List<String>, rest: String = ""): String {
  val more = options.size - CHOICE_MAX
  val tail = if (more <= 0) "" else " ; et $more ${if (more == 1) "autre" else "autres"}" + if (rest.isEmpty()) "" else ", $rest"
  return "$intro : " + options.take(CHOICE_MAX).mapIndexed { i, o -> "${CHOICE_WORDS[i]}, $o" }.joinToString(" ; ") + "$tail. Lequel ?"
}

fun fromSender(said: String, contact: Contact?, sender: String): Boolean {
  val s = fold(sender)
  if (s.isEmpty()) return false
  if (contact != null) return s == fold(contact.name) || contact.numbers.any { samePhone(it, sender) }
  val words = fold(said).split(" ").filter { it.isNotEmpty() }
  return words.isNotEmpty() && s.split(" ").let { parts -> words.all { it in parts } }
}

fun heardText(order: String): String = "« ${order.trim()} »"

fun unreadLines(read: List<Heard>, more: Int, notesMissing: Boolean, smsRead: Boolean, now: Long = 0): List<String> {
  if (read.isEmpty() && !notesMissing) return announceMessages(emptyList())
  val lines = ArrayList<String>()
  if (read.isEmpty()) { if (smsRead) lines.add("Aucun nouveau SMS.") } else {
    lines.addAll(announceMessages(read, now))
    if (more > 0) lines.add("Il en reste $more.")
  }
  if (notesMissing) lines.add(NOTES_MISSING)
  return lines
}

fun sentText(s: Sent): String = when (s) {
  Sent.OK -> "Message envoyé."
  Sent.FAILED -> "Envoi impossible."
  Sent.UNKNOWN -> "Envoi non confirmé. Vérifiez avant de réessayer."
}

fun noteReplyText(r: NoteReply?): String = when (r) {
  NoteReply.SENT -> "Message envoyé."
  NoteReply.GONE -> "Ce message n'est plus disponible."
  else -> "Envoi impossible."
}

fun confirmStep(answer: Answer, asked: Int): Confirm = when (answer) {
  Answer.SEND -> Confirm.SEND
  Answer.RETRY -> Confirm.RETRY
  Answer.CANCEL -> Confirm.CANCEL
  Answer.UNKNOWN -> if (asked < 2) Confirm.ASK else Confirm.CANCEL
}

fun review(to: ReplyTo, text: String): String = when (to) {
  is ReplyTo.Text -> "SMS pour ${to.name}${if (to.several) ", au ${speakableNumber(to.number)}" else ""} : $text. Envoyer, annuler ou recommencer ?"
  is ReplyTo.Note -> "Réponse à ${to.name} sur ${to.label} : $text. Envoyer, annuler ou recommencer ?"
}

fun musicFailure(listener: Boolean, launch: Boolean = true): String = when {
  !listener -> "Je n'ai rien à piloter. Accordez l'accès aux notifications dans Estompe."
  !launch -> "L'application de musique n'a pas répondu. Pour qu'Estompe puisse l'ouvrir, autorisez l'affichage par-dessus les autres applications."
  else -> "L'application de musique n'a pas répondu."
}

class AssistantStore(context: Context) {
  private val prefs = context.applicationContext.getSharedPreferences("assistant", Context.MODE_PRIVATE)

  fun load() = AssistantSettings(
    scope = scopeOf(prefs.getString("scope", "")) ?: AssistantScope.OFF,
    apps = prefs.getStringSet("apps", null).orEmpty().toSet(),
    musicApp = prefs.getString("musicApp", "").orEmpty(),
    llm = prefs.getBoolean("llm", false),
  )

  fun toJson(s: AssistantSettings = load()): JSONObject = settingsJson(s)

  fun recording(): Boolean = BuildConfig.DEBUG && prefs.getBoolean("record", false)

  fun setRecording(on: Boolean) { prefs.edit().putBoolean("record", BuildConfig.DEBUG && on).apply() }

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
  private lateinit var overlay: AssistantOverlay
  private var settings = AssistantSettings()
  private var apps: List<MusicApp> = emptyList()
  private var started = false
  private var destroyed = false
  private var busy = false
  private var busySince = 0L
  private var turn = 0
  private var notice: String? = null
  private var failures = 0
  private var last: ReplyTo? = null
  private val spoken = LinkedHashSet<String>()
  private var llm: Llm? = null
  private val tones = HashSet<ToneGenerator>()
  private var awake: PowerManager.WakeLock? = null
  private var pendingAsk = false
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
    overlay = AssistantOverlay(this)
    if (BuildConfig.DEBUG) {
      hearTrace = { DriveService.note("assistant : $it") }
      val store = AssistantStore(this)
      hearDump = { label, samples, count -> if (store.recording()) record(label, samples.copyOf(count)) }
    }
    voice = Voice(this, DriveStore(this).load(), {
      if (notice != null) { notice = null; if (started && !destroyed) status() }
    }) { text ->
      DriveService.note("assistant : $text")
      notice = text
      if (started && !destroyed) status()
    }
    Mic.freed = freed
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      starting = false
      listening = false
      if (!started) foreground(false)
      cancelPause(this)
      stopSelf()
      return START_NOT_STICKY
    }
    val ask = intent?.action == ACTION_ASK
    if (ask) pendingAsk = true
    if (started) foreground(true) else begin()
    starting = false
    if (ask) ask(0)
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    destroyed = true
    alive = false
    listening = false
    if (busy) Mic.converse(false)
    if (Mic.freed === freed) Mic.freed = null
    hearTrace = null
    hearDump = null
    ui.removeCallbacksAndMessages(null)
    try { awake?.let { if (it.isHeld) it.release() } } catch (e: Exception) { DriveService.note("assistant : libération du verrou d'éveil ${e.javaClass.simpleName}") }
    awake = null
    overlay.hide()
    voice.shutdown()
    closeLlm()
    for (t in tones) try { t.release() } catch (e: Exception) {}
    tones.clear()
    super.onDestroy()
  }

  private fun begin() {
    val why = blocked(this) ?: hidden()
    if (why == null && foreground(true)) {
      started = true
      keepAwake()
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

  private fun keepAwake() {
    if (awake != null) return
    try {
      awake = getSystemService(PowerManager::class.java)?.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "estompe:assistant")?.apply { setReferenceCounted(false); acquire() }
    } catch (e: Exception) {
      DriveService.note("assistant : verrou d'éveil ${e.javaClass.simpleName}")
    }
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
      .setContentTitle("Assistant vocal").setContentText(text ?: notice ?: if (wanted()) "Dis “Estompe” pour donner un ordre." else "Ordre en cours.")
      .setOngoing(true).setOnlyAlertOnce(true).setSilent(true).setPriority(NotificationCompat.PRIORITY_LOW).setContentIntent(open)
      .addAction(0, "Arrêter", stop).build()
  }

  private fun status(text: String? = null) {
    try { NotificationManagerCompat.from(this).notify(NOTIF_ID, notification(text)) } catch (e: SecurityException) { DriveService.note("assistant : notification refusée") }
  }

  private fun check() {
    if (busy && SystemClock.elapsedRealtime() - busySince > BUSY_MAX_MS) {
      DriveService.note("assistant : ordre bloqué, retour à la veille")
      voice.cancel()
      done()
    }
    if (!busy && !pendingAsk && !wanted()) {
      listening = false
      stopSelf()
      return
    }
    if (!VoiceService.running) return resume()
    voice.unwatch()
    listening = false
  }

  private fun resume() {
    if (destroyed || !started || busy || Mic.busy || VoiceService.running || !wanted()) return
    voice.watch(::heard) { on, failed ->
      listening = on
      if (failed && !destroyed && ++failures >= MAX_FAILURES) {
        pause(this, PAUSE_BUSY)
        stopSelf()
      }
    }
  }

  private fun wanted() = shouldRun(AssistantStore(this).load(), DriveService.running)

  private fun ask(tries: Int) {
    if (destroyed || !started || busy || VoiceService.running) { pendingAsk = false; return }
    if (!voice.isReady && tries < ASK_TRIES) { ui.postDelayed({ ask(tries + 1) }, ASK_WAIT_MS); return }
    pendingAsk = false
    wake("")
  }

  private fun heard(text: String) {
    failures = 0
    wake(wakeWord(text) ?: text)
  }

  private fun wake(order: String) {
    if (busy || destroyed) return
    if (!voice.isReady) {
      DriveService.note("assistant : ordre refusé, synthèse vocale pas prête")
      tone(ToneGenerator.TONE_PROP_NACK)
      if (notice == null) notice = TTS_WAIT
      status()
      overlay.show(TTS_WAIT)
      overlay.hide(HIDE_MS)
      return
    }
    busy = true
    busySince = SystemClock.elapsedRealtime()
    turn++
    Mic.converse(true)
    voice.unwatch()
    settings = AssistantStore(this).load()
    voice.focus()
    if (order.isNotBlank()) return execute(order)
    say(PROMPT) { said ->
      if (!said) return@say mute()
      listen(ORDER_MS, known = voice.phrases) { t -> if (t.isNullOrBlank()) done() else execute(t) }
    }
  }

  private fun execute(text: String, again: Boolean = false) {
    overlay.show(heardText(text), OverlayState.HEARD)
    status("Entendu : ${heardText(text)}")
    val c = try { parseCommand(text, apps) } catch (e: Exception) {
      DriveService.note("assistant : analyse impossible (${e.javaClass.simpleName})")
      Command.Unknown
    }
    trace("entendu « $text »", c)
    if (c == Command.Unknown) understand(text, again) else run(c, again)
  }

  private fun trace(what: String, c: Command) {
    if (!BuildConfig.DEBUG) return
    val s = c.toString()
    DriveService.note("assistant : $what → ${c.javaClass.simpleName}${if ('(' in s) s.substring(s.indexOf('(')) else ""}")
  }

  private fun understand(text: String, again: Boolean) {
    if (!settings.llm || !LlmModel.AVAILABLE || !LlmModel.supported() || !LlmModel.ready(this)) return run(Command.Unknown, again)
    ui.removeCallbacks(unload)
    val model = llm ?: Llm(this).also { llm = it }
    var answered = false
    val mine = turn
    val wait = Runnable { if (!answered && !destroyed && mine == turn) voice.say("Un instant.") }
    ui.postDelayed(wait, WAIT_MS)
    model.understand(text, apps) { c ->
      ui.removeCallbacks(wait)
      ui.removeCallbacks(unload)
      ui.postDelayed(unload, LLM_IDLE_MS)
      if (answered || destroyed || mine != turn) return@understand
      answered = true
      trace("modèle de langue", c)
      guard { run(c, again) }
    }
  }

  private fun askAgain() {
    say(REPEAT) { said ->
      if (!said) return@say mute()
      listen(ORDER_MS, known = voice.phrases) { t -> if (t.isNullOrBlank()) done() else execute(t, true) }
    }
  }

  private fun run(c: Command, again: Boolean = false) {
    try {
      when (c) {
        Command.Unknown -> if (again) finish(NOT_UNDERSTOOD) else askAgain()
        Command.Help -> finish(HELP_TEXT)
        Command.Cancel -> finish("D'accord.")
        Command.ReadUnread -> readUnread()
        is Command.ReadFrom -> readFrom(c.contact, c.single)
        Command.Reply -> reply()
        is Command.SendSms -> sendSms(c.contact, c.text)
        is Command.Music -> background({ music.transport(c.action, c.app, settings.musicApp) }) { played(it == true) }
        is Command.PlaySearch -> music.search(c.query, c.app.ifEmpty { settings.musicApp }, current { ok: Boolean -> played(ok) })
        is Command.PlayPlaylist -> music.playlist(c.name, c.app.ifEmpty { settings.musicApp }, current { ok: Boolean -> played(ok) })
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
    if (ok) done() else finish(musicFailure(AssistantListener.enabled(this), music.canLaunch()))
  }

  private fun readUnread() {
    val sms = phone.canReadSms()
    val notes = AssistantListener.enabled(this)
    if (!sms && !notes) return finish("Autorisez les SMS ou l'accès aux notifications dans Estompe.")
    val chosen = settings.apps
    background({ Triple(if (sms) phone.unreadSms() else emptyList(), if (notes) AssistantListener.pending(chosen) else emptyList(),
      notes && chosen.isNotEmpty() && !AssistantListener.connected) }) { r ->
      val items = mergeUnread(r?.first.orEmpty(), r?.second.orEmpty(), spoken)
      val batch = items.takeLast(READ_MAX)
      batch.lastOrNull()?.let { last = it.to }
      sayAll(unreadLines(batch.map { it.heard }, items.size - batch.size, r?.third == true, sms, System.currentTimeMillis())) { all ->
        if (all) {
          val ids = batch.mapNotNull { it.sms }
          if (ids.isNotEmpty()) thread { phone.markSpoken(ids) }
          for (u in batch) {
            (u.to as? ReplyTo.Note)?.let { remember(it.key) }
            u.notes.forEach { remember(it) }
          }
        }
        done()
      }
    }
  }

  private fun readFrom(name: String, single: Boolean) {
    val sms = phone.canReadSms() && phone.canReadContacts()
    val notes = AssistantListener.enabled(this)
    if (!sms && !notes) return finish("Autorisez les SMS et les contacts, ou l'accès aux notifications, dans Estompe.")
    val chosen = settings.apps
    background({
      val active = if (notes) AssistantListener.pending(chosen) else emptyList()
      (if (sms) matchContacts(name, phone.contacts()) else emptyList()) to (active + AssistantListener.remembered(this, chosen, active))
    }) { r ->
      val found = r?.first.orEmpty()
      val pending = r?.second.orEmpty()
      if (found.isEmpty()) {
        val items = mergeUnread(emptyList(), pending.filter { fromSender(name, null, it.sender) }, emptySet())
        return@background if (items.isEmpty()) finish("Je ne trouve aucun message de $name.") else readItems(items[0].heard.sender, items, single)
      }
      pickContact(name, found) { c ->
        background({ phone.smsFrom(c).also { found -> if (BuildConfig.DEBUG) traceContact(c, found, pending) } }) { list ->
          readItems(c.name, mergeUnread(list.orEmpty(), pending.filter { fromSender(name, c, it.sender) }, emptySet()), single,
            c.numbers.firstOrNull { it.isNotBlank() }?.let { ReplyTo.Text(it, c.name) })
        }
      }
    }
  }

  private fun traceContact(c: Contact, sms: List<Sms>, pending: List<Pending>) {
    val day = SimpleDateFormat("dd/MM/yyyy HH:mm", Locale.FRANCE)
    val mms = phone.mmsFrom(c)
    DriveService.note("assistant : contact « ${c.name} », ${c.numbers.size} numéro(s), ${sms.size} SMS reçus" +
      (sms.lastOrNull()?.let { ", le plus récent du ${day.format(Date(it.date))}" } ?: "") +
      ", ${mms.first} MMS récents" + (if (mms.first > 0) ", le plus récent du ${day.format(Date(mms.second))}" else "") +
      ", ${pending.count { fromSender(c.name, c, it.sender) }} notification(s) en attente, applications lues : ${settings.apps.size}")
  }

  private fun readItems(name: String, items: List<Unread>, single: Boolean, bySms: ReplyTo.Text? = null) {
    val batch = items.takeLast(if (single) 1 else READ_MAX)
    if (batch.isEmpty()) return finish("Aucun message de $name.")
    val to = batch.last().to
    last = if (to is ReplyTo.Note && !to.replyable && bySms != null && to.label == AssistantListener.smsLabel(this)) bySms else to
    sayAll(announceMessages(batch.map { it.heard }, System.currentTimeMillis())) { done() }
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
    if (!phone.canReadContacts()) return finish("Autorisez l'accès aux contacts dans Estompe.")
    background({ splitRecipient(name, phone.contacts()) }) { r ->
      val rest = r?.second.orEmpty().trim()
      val body = text.ifBlank { rest }
      pickContact(name, if (text.isNotBlank() && rest.isNotEmpty()) emptyList() else r?.first.orEmpty()) { c -> number(c) { n, several -> compose(ReplyTo.Text(n, c.name, several), body) } }
    }
  }

  private fun pickContact(name: String, found: List<Contact>, then: (Contact) -> Unit) {
    when (found.size) {
      0 -> finish("Je ne trouve pas $name dans vos contacts.")
      1 -> then(found[0])
      else -> choose("Plusieurs contacts", found.map { it.name }, "précisez le nom") { then(found[it]) }
    }
  }

  private fun number(c: Contact, then: (String, Boolean) -> Unit) {
    val numbers = c.numbers.filter { it.isNotBlank() }
    when (numbers.size) {
      0 -> finish("Aucun numéro pour ${c.name}.")
      1 -> then(numbers[0], false)
      else -> choose("Plusieurs numéros pour ${c.name}", numbers.map { Phone.speakable(it) }) { then(numbers[it], true) }
    }
  }

  private fun choose(intro: String, options: List<String>, rest: String = "", asked: Int = 1, picked: (Int) -> Unit) {
    val shown = minOf(options.size, CHOICE_MAX)
    say(choices(intro, options, rest)) { said ->
      if (!said) return@say mute()
      listen(CHOICE_MS, known = CHOICE_PHRASES) { heard ->
        val t = heard.orEmpty()
        val i = pickChoice(t, shown)
        when {
          i != null -> picked(i)
          parseCommand(t) == Command.Cancel -> finish("D'accord.")
          asked < 2 -> choose(intro, options, rest, asked + 1, picked)
          else -> finish("Je n'ai pas compris, j'abandonne.")
        }
      }
    }
  }

  private fun compose(to: ReplyTo, text: String) {
    if (text.isBlank()) dictate(to) else confirm(to, text, 1)
  }

  private fun dictate(to: ReplyTo) {
    say("Quel est le message ?") { said ->
      if (!said) return@say mute()
      listen(DICTATE_MS, true) { t ->
        when {
          t.isNullOrBlank() -> finish("Je n'ai rien entendu.")
          parseCommand(t) == Command.Cancel -> finish("Message annulé.")
          else -> confirm(to, t, 1)
        }
      }
    }
  }

  private fun confirm(to: ReplyTo, text: String, asked: Int) {
    voice.focus(true)
    say(review(to, text)) { said ->
      if (!said) return@say mute()
      listen(CONFIRM_MS, true, CONFIRM_PHRASES) { answer ->
        when (confirmStep(confirmAnswer(answer.orEmpty()), asked)) {
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
      is ReplyTo.Text -> phone.send(to.number, text, current { s: Sent -> finish(sentText(s)) })
      is ReplyTo.Note -> background({ AssistantListener.reply(to.key, to.name, text) }) { r -> finish(noteReplyText(r)) }
    }
  }

  private fun mute() {
    DriveService.note("assistant : phrase non prononcée, ordre abandonné")
    tone(ToneGenerator.TONE_PROP_NACK)
    if (notice == null) notice = TTS_LOST
    status()
    done()
  }

  private fun say(text: String, display: Boolean = true, then: (Boolean) -> Unit) {
    val mine = turn
    if (display) { overlay.show(text); status(SPEAKING) }
    busySince = maxOf(busySince, SystemClock.elapsedRealtime()) + voice.guardMs(text)
    voice.say(text) { said ->
      if (destroyed || mine != turn) return@say
      busySince = SystemClock.elapsedRealtime()
      guard { then(said) }
    }
  }

  private fun listen(ms: Int, pause: Boolean = false, known: Set<String>? = null, result: (String?) -> Unit) {
    val mine = turn
    overlay.show(LISTENING, OverlayState.LISTEN)
    status(LISTENING)
    voice.listen(ms, false, pause, known) { t ->
      if (destroyed || mine != turn) return@listen
      busySince = SystemClock.elapsedRealtime()
      guard { result(t) }
    }
  }

  private fun sayAll(lines: List<String>, then: (Boolean) -> Unit) {
    if (lines.isEmpty()) return then(true)
    var all = true
    lines.forEachIndexed { i, line ->
      say(line, i == 0) { said ->
        if (!said) all = false
        if (i == lines.lastIndex) then(all) else overlay.show(lines[i + 1])
      }
    }
  }

  private fun finish(text: String) {
    say(text) { done() }
  }

  private fun done() {
    if (destroyed) return
    busy = false
    turn++
    Mic.converse(false)
    voice.abandonFocus()
    overlay.hide(HIDE_MS)
    status()
    refreshApps()
    ui.removeCallbacks(resumeLater)
    ui.postDelayed(resumeLater, RESUME_MS)
  }

  private fun guard(block: () -> Unit) {
    try { block() } catch (e: Exception) { failed(e) }
  }

  private fun <T> current(then: (T) -> Unit): (T) -> Unit {
    val mine = turn
    return { v -> ui.post { if (!destroyed && mine == turn) guard { then(v) } } }
  }

  private fun <T> background(job: () -> T, then: (T?) -> Unit) {
    val back = current(then)
    thread {
      val r = try { job() } catch (e: Exception) {
        DriveService.note("assistant : lecture impossible (${e.javaClass.simpleName})")
        null
      }
      back(r)
    }
  }

  private fun refreshApps() {
    thread {
      val found = music.apps()
      val names = try { phone.contacts().map { it.name } } catch (e: Exception) { emptyList() }
      val known = commandPhrases(names, found.map { it.label })
      ui.post { if (!destroyed) { apps = found; voice.phrases = known } }
    }
  }

  private fun remember(key: String) {
    spoken.add(key)
    while (spoken.size > SPOKEN_MAX) spoken.remove(spoken.first())
  }

  private fun tone(kind: Int) {
    val t = try { ToneGenerator(AudioManager.STREAM_MUSIC, 80) } catch (e: Exception) {
      DriveService.note("assistant : signal sonore ${e.javaClass.simpleName}")
      return
    }
    tones.add(t)
    try { t.startTone(kind, TONE_MS) } catch (e: Exception) { DriveService.note("assistant : signal sonore ${e.javaClass.simpleName}") }
    ui.postDelayed({ if (tones.remove(t)) try { t.release() } catch (e: Exception) {} }, TONE_MS + 250L)
  }

  private fun record(label: String, samples: ShortArray) {
    thread {
      try {
        val dir = recordings(this) ?: return@thread
        dir.mkdirs()
        val name = SimpleDateFormat("yyyyMMdd-HHmmss-SSS", Locale.US).format(Date()) + "-$label.wav"
        val data = ByteBuffer.allocate(44 + samples.size * 2).order(ByteOrder.LITTLE_ENDIAN)
        data.put("RIFF".toByteArray()).putInt(36 + samples.size * 2).put("WAVEfmt ".toByteArray()).putInt(16).putShort(1).putShort(1)
          .putInt(HEAR_RATE).putInt(HEAR_RATE * 2).putShort(2).putShort(16).put("data".toByteArray()).putInt(samples.size * 2)
        for (s in samples) data.putShort(s)
        File(dir, name).writeBytes(data.array())
        dir.listFiles()?.sortedBy { it.name }?.dropLast(RECORD_MAX)?.forEach { it.delete() }
      } catch (e: Exception) {
        DriveService.note("assistant : enregistrement de diagnostic ${e.javaClass.simpleName}")
      }
    }
  }

  private fun closeLlm() {
    ui.removeCallbacks(unload)
    try { llm?.close() } catch (e: Exception) { DriveService.note("assistant : libération du modèle de langue ${e.javaClass.simpleName}") }
    llm = null
  }

  companion object {
    const val ACTION_SYNC = "fr.plbls.estompe.assistant.SYNC"
    const val ACTION_STOP = "fr.plbls.estompe.assistant.STOP"
    const val ACTION_ASK = "fr.plbls.estompe.assistant.ASK"
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
    private const val TONE_MS = 150
    private const val HIDE_MS = 2_500L
    private const val ASK_TRIES = 25
    private const val RECORD_MAX = 60
    private const val ASK_WAIT_MS = 200L
    private const val SPOKEN_MAX = 300
    private const val MAX_FAILURES = 5
    private const val BUSY_MAX_MS = 90_000L
    private const val TTS_WAIT = "Synthèse vocale pas encore prête : ordre ignoré."
    private const val TTS_LOST = "Synthèse vocale interrompue : ordre abandonné, aucun message envoyé."
    private const val PAUSE_BACKGROUND = "Assistant vocal en pause : touche pour relancer l'écoute."
    private const val PAUSE_MIC = "Assistant vocal en pause : autorise le micro dans Estompe."
    private const val PAUSE_MODEL = "Assistant vocal en pause : télécharge le modèle de reconnaissance dans Estompe."
    private const val PAUSE_BUSY = "Assistant vocal en pause : micro indisponible, touche pour relancer l'écoute."

    @Volatile var listening = false
    @Volatile private var alive = false
    @Volatile private var starting = false

    fun sync(c: Context) {
      try {
        if (!shouldRun(AssistantStore(c).load(), DriveService.running)) { if (alive) return; return stop(c) }
        if (alive) return
        (blocked(c) ?: hidden())?.let { listening = false; return pause(c, it) }
        starting = true
        ContextCompat.startForegroundService(c, Intent(c, AssistantService::class.java).setAction(ACTION_SYNC))
      } catch (e: Exception) {
        starting = false
        DriveService.note("démarrage de l'assistant vocal : ${e.javaClass.simpleName}")
        listening = false
        pause(c, PAUSE_BACKGROUND)
      }
    }

    fun ask(c: Context): Boolean {
      try {
        if (blocked(c) != null) return false
        starting = true
        ContextCompat.startForegroundService(c, Intent(c, AssistantService::class.java).setAction(ACTION_ASK))
        return true
      } catch (e: Exception) {
        starting = false
        DriveService.note("assistant sur demande : ${e.javaClass.simpleName}")
        return false
      }
    }

    fun recordings(c: Context): File? = try { c.getExternalFilesDir("assistant") } catch (e: Exception) { null }

    fun assistantRole(c: Context): Boolean = try {
      Build.VERSION.SDK_INT >= 29 && c.getSystemService(RoleManager::class.java)?.isRoleHeld(RoleManager.ROLE_ASSISTANT) == true
    } catch (e: Exception) { false }

    fun stop(c: Context) {
      listening = false
      cancelPause(c)
      val service = Intent(c, AssistantService::class.java)
      try {
        if (starting) c.startService(Intent(service).setAction(ACTION_STOP)) else c.stopService(service)
      } catch (e: Exception) {
        DriveService.note("arrêt de l'assistant vocal : ${e.javaClass.simpleName}")
        try { c.stopService(service) } catch (e2: Exception) { DriveService.note("arrêt de l'assistant vocal : ${e2.javaClass.simpleName}") }
      }
    }

    fun unrestricted(c: Context): Boolean = try {
      c.getSystemService(PowerManager::class.java)?.isIgnoringBatteryOptimizations(c.packageName) == true
    } catch (e: Exception) { false }

    private fun blocked(c: Context): String? = when {
      ContextCompat.checkSelfPermission(c, Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED -> PAUSE_MIC
      !VoskModel.ready(c) -> PAUSE_MODEL
      else -> null
    }

    private fun hidden(): String? = if (Build.VERSION.SDK_INT >= 30 && !appInForeground()) PAUSE_BACKGROUND else null

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
