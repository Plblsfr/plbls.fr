package fr.plbls.estompe

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import org.json.JSONObject
import org.vosk.Model
import org.vosk.Recognizer
import org.vosk.android.RecognitionListener
import org.vosk.android.SpeechService
import java.util.Locale
import kotlin.concurrent.thread

object VoskShared {
  private var model: Model? = null
  private var users = 0

  @Synchronized fun acquire(c: Context): Model? {
    val m = model ?: try { Model(VoskModel.path(c)) } catch (e: Throwable) { DriveService.note("chargement du modèle : $e"); return null }
    model = m
    users++
    return m
  }

  @Synchronized fun release() {
    if (users > 0) users--
    if (users > 0) return
    try { model?.close() } catch (e: Throwable) { DriveService.note("libération du modèle : ${e.javaClass.simpleName}") }
    model = null
  }
}

object Mic {
  var hearing = false
    private set
  var dialog = false
    private set
  private var talking = 0
  internal var watcher: Voice? = null
  var freed: (() -> Unit)? = null
  private val waiting = LinkedHashSet<() -> Unit>()
  val busy get() = hearing || talking > 0
  val quiet get() = !hearing && !dialog

  internal fun take(listen: Boolean) {
    if (listen) hearing = true else talking++
    watcher?.preempt()
  }

  internal fun give(listen: Boolean) {
    if (listen) hearing = false else if (talking > 0) talking--
    if (!busy) freed?.invoke()
    if (listen) wake()
  }

  fun converse(on: Boolean) {
    dialog = on
    if (!on) wake()
  }

  fun whenQuiet(then: () -> Unit) {
    if (quiet) then() else waiting.add(then)
  }

  fun forget(then: () -> Unit) { waiting.remove(then) }

  private fun wake() {
    if (!quiet || waiting.isEmpty()) return
    val all = waiting.toList()
    waiting.clear()
    for (w in all) try { w() } catch (e: Exception) { DriveService.note("micro : rappel ${e.javaClass.simpleName}") }
  }
}

private fun voskText(h: String?) = try { h?.let { JSONObject(it).optString("text") }?.takeIf { it.isNotBlank() } } catch (e: Exception) { null }

fun utteranceGuardMs(text: String): Long = 5_000L + 120L * text.length

private class WakeLoop(private val model: Model, private val heard: (String) -> Unit, private val failed: (String) -> Unit) {
  @Volatile private var running = true
  private var worker: Thread? = null

  fun start() { worker = thread(name = "estompe-wake") { loop() } }

  fun stop() {
    running = false
    val w = worker ?: return
    worker = null
    if (w === Thread.currentThread()) return
    try { w.join(JOIN_MS) } catch (e: InterruptedException) { Thread.currentThread().interrupt() }
  }

  private fun loop() {
    var record: AudioRecord? = null
    var recognizer: Recognizer? = null
    var error: String? = null
    try {
      val min = AudioRecord.getMinBufferSize(RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
      if (min <= 0) throw IllegalStateException("tampon $min")
      val buf = ShortArray(RATE / 10)
      val r = AudioRecord(MediaRecorder.AudioSource.VOICE_RECOGNITION, RATE, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT, maxOf(min, buf.size * 2) * 2)
      record = r
      if (r.state != AudioRecord.STATE_INITIALIZED) throw IllegalStateException("micro non initialisé")
      val rec = Recognizer(model, RATE.toFloat())
      recognizer = rec
      r.startRecording()
      if (r.recordingState != AudioRecord.RECORDSTATE_RECORDING) throw IllegalStateException("micro occupé")
      var empty = 0
      while (running) {
        val n = r.read(buf, 0, buf.size)
        if (n < 0) throw IllegalStateException("lecture du micro $n")
        if (n == 0) { if (++empty > MAX_EMPTY) throw IllegalStateException("micro muet"); continue }
        empty = 0
        if (rec.acceptWaveForm(buf, n) && running) voskText(rec.result)?.let(heard)
      }
    } catch (t: Throwable) {
      if (running) error = t.javaClass.simpleName + (t.message?.let { " $it" } ?: "")
    } finally {
      try { record?.stop() } catch (t: Throwable) {}
      try { record?.release() } catch (t: Throwable) {}
      try { recognizer?.close() } catch (t: Throwable) {}
    }
    error?.let(failed)
  }

  companion object {
    private const val RATE = 16000
    private const val JOIN_MS = 600L
    private const val MAX_EMPTY = 50
  }
}

class Voice(private val context: Context, private val settings: DriveSettings, private val ready: () -> Unit, private val noEngine: (String) -> Unit) {
  private val ui = Handler(Looper.getMainLooper())
  private val audio = context.getSystemService(AudioManager::class.java)
  private val done = LinkedHashMap<String, (Boolean) -> Unit>()
  private val guards = HashMap<String, Runnable>()
  private var tts: TextToSpeech? = null
  private var engine = 0
  private var utterance = 0
  private var talking = false
  private var hearing = false
  private var routed = false
  private var turn = 0
  @Volatile private var closed = false
  private var focus: Any? = null
  private var exclusive = false
  private var model: Model? = null
  private var speech: SpeechService? = null
  private var recognizer: Recognizer? = null
  private var wake: WakeLoop? = null
  private var watchWanted = false
  private var watchToken = 0
  private var watchChanged: ((Boolean, Boolean) -> Unit)? = null

  var isReady = false
    private set
  val watching get() = wake != null

  init { create() }

  private fun create() {
    val gen = ++engine
    tts = try {
      TextToSpeech(context, { code -> ui.post { onInit(gen, code) } }, settings.engine.ifEmpty { null })
    } catch (e: Exception) {
      DriveService.note("synthèse vocale : ${e.javaClass.simpleName}")
      null
    }
    if (tts == null) ui.post { if (!closed && gen == engine) noEngine("Aucun moteur de synthèse vocale : touche pour en installer un.") }
  }

  private fun onInit(gen: Int, code: Int) {
    if (closed || gen != engine) return
    val t = tts
    if (code != TextToSpeech.SUCCESS || t == null) return noEngine("Aucun moteur de synthèse vocale : touche pour en installer un.")
    try {
      if (t.setLanguage(Locale.FRENCH) < TextToSpeech.LANG_AVAILABLE) return noEngine("Voix française absente : touche pour l'installer.")
      t.setSpeechRate(settings.safeRate)
      t.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANCE_NAVIGATION_GUIDANCE)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
      t.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
        override fun onStart(id: String?) {}
        override fun onDone(id: String?) { ui.post { if (gen == engine) id?.let { finished(it, true) } } }
        @Deprecated("API < 21") override fun onError(id: String?) { ui.post { if (gen == engine) id?.let { failed(it) } } }
        override fun onError(id: String?, code: Int) { DriveService.note("synthèse vocale : erreur $code"); onError(id) }
        override fun onStop(id: String?, interrupted: Boolean) { ui.post { if (gen == engine) id?.let { finished(it, false) } } }
      })
    } catch (e: Exception) {
      DriveService.note("synthèse vocale : réglage impossible (${e.javaClass.simpleName})")
      return noEngine("Synthèse vocale indisponible : touche pour vérifier le moteur.")
    }
    isReady = true
    ready()
  }

  fun say(text: String, then: (Boolean) -> Unit = {}) {
    val t = tts
    if (t == null || !isReady || closed) return then(false)
    val id = "u${utterance++}"
    done[id] = then
    if (!talking) { talking = true; Mic.take(false) }
    focus()
    val guard = Runnable { DriveService.note("synthèse vocale : phrase sans fin, moteur relancé"); failed(id) }
    guards[id] = guard
    ui.postDelayed(guard, utteranceGuardMs(text))
    val code = try { t.speak(text, TextToSpeech.QUEUE_ADD, null, id) } catch (e: Exception) { TextToSpeech.ERROR }
    if (code != TextToSpeech.SUCCESS) { DriveService.note("synthèse vocale : refus de lire"); failed(id) }
  }

  private fun failed(id: String) {
    if (closed || id !in done) return
    finished(id, false)
    restart()
  }

  private fun restart() {
    if (closed) return
    val old = tts
    tts = null
    isReady = false
    try { old?.stop() } catch (e: Exception) {}
    try { old?.shutdown() } catch (e: Exception) { DriveService.note("synthèse vocale : ${e.javaClass.simpleName}") }
    for (id in done.keys.toList()) finished(id, false)
    if (!closed) create()
  }

  private fun finished(id: String, spoken: Boolean) {
    if (closed) return
    guards.remove(id)?.let { ui.removeCallbacks(it) }
    val then = done.remove(id) ?: return
    try { then(spoken) } catch (e: Exception) { DriveService.note("synthèse vocale : rappel ${e.javaClass.simpleName}") }
    if (done.isEmpty() && talking && !closed) { talking = false; Mic.give(false) }
  }

  fun listen(ms: Int, bluetooth: Boolean = true, exclusive: Boolean = false, result: (String?) -> Unit) {
    if (closed) return
    if (Mic.hearing) {
      DriveService.note("micro occupé par une autre écoute")
      ui.post { if (!closed) result(null) }
      return
    }
    if (exclusive) focus(true)
    val mine = turn
    hearing = true
    Mic.take(true)
    thread {
      val loaded = loadModel()
      ui.post {
        if (closed || mine != turn) return@post
        if (loaded == null) { hearing = false; Mic.give(true); return@post result(null) }
        if (bluetooth) micRoute(true)
        var over = false
        fun end(text: String?) {
          if (over || closed || mine != turn) return
          over = true
          stopListening()
          if (bluetooth) micRoute(false)
          hearing = false
          Mic.give(true)
          try { result(text) } catch (e: Exception) { DriveService.note("reconnaissance vocale : rappel ${e.javaClass.simpleName}") }
        }
        ui.postDelayed({
          if (closed || mine != turn) return@postDelayed
          try {
            val r = Recognizer(loaded, 16000f)
            recognizer = r
            val s = SpeechService(r, 16000f)
            speech = s
            s.startListening(object : RecognitionListener {
              override fun onPartialResult(hypothesis: String?) {}
              override fun onResult(hypothesis: String?) { end(voskText(hypothesis)) }
              override fun onFinalResult(hypothesis: String?) { end(voskText(hypothesis)) }
              override fun onError(e: Exception?) { DriveService.note("reconnaissance vocale : $e"); end(null) }
              override fun onTimeout() { end(null) }
            }, ms)
          } catch (e: Exception) {
            DriveService.note("micro : $e")
            end(null)
          }
        }, if (bluetooth) 700L else 300L)
      }
    }
  }

  fun cancel() {
    if (closed) return
    turn++
    stopListening()
    if (routed) micRoute(false)
    if (hearing) { hearing = false; Mic.give(true) }
    for (g in guards.values) ui.removeCallbacks(g)
    guards.clear()
    done.clear()
    try { tts?.stop() } catch (e: Exception) { DriveService.note("synthèse vocale : ${e.javaClass.simpleName}") }
    if (talking) { talking = false; Mic.give(false) }
    abandonFocus()
  }

  fun watch(heard: (String) -> Unit, changed: (Boolean, Boolean) -> Unit) {
    if (closed || watchWanted || Mic.busy) return
    watchWanted = true
    val token = ++watchToken
    watchChanged = changed
    Mic.watcher = this
    thread {
      val loaded = loadModel()
      ui.post {
        if (closed || token != watchToken) return@post
        if (loaded == null || Mic.busy) return@post stopWatch(loaded == null)
        val loop = WakeLoop(loaded, { text -> ui.post { if (!closed && token == watchToken) heard(text) } }) { why ->
          ui.post {
            if (closed || token != watchToken) return@post
            DriveService.note("veille du mot d'éveil : $why")
            stopWatch(true)
          }
        }
        wake = loop
        try { loop.start() } catch (e: Throwable) {
          DriveService.note("micro de veille : ${e.javaClass.simpleName}")
          return@post stopWatch(true)
        }
        changed(true, false)
      }
    }
  }

  fun unwatch() = stopWatch(false)

  internal fun preempt() = stopWatch(false)

  private fun stopWatch(failed: Boolean) {
    if (!watchWanted) return
    watchWanted = false
    watchToken++
    if (Mic.watcher === this) Mic.watcher = null
    val w = wake
    wake = null
    w?.stop()
    val c = watchChanged
    watchChanged = null
    c?.invoke(false, failed)
  }

  private fun loadModel(): Model? {
    synchronized(this) { if (closed) return null; model?.let { return it } }
    val m = VoskShared.acquire(context) ?: return null
    synchronized(this) { if (!closed && model == null) { model = m; return m } }
    VoskShared.release()
    return synchronized(this) { if (closed) null else model }
  }

  private fun stopListening() {
    val s = speech
    val r = recognizer
    speech = null
    recognizer = null
    try { s?.stop(); s?.shutdown() } catch (e: Exception) { DriveService.note("micro : ${e.javaClass.simpleName}") }
    try { r?.close() } catch (e: Exception) { DriveService.note("micro : ${e.javaClass.simpleName}") }
  }

  @Suppress("DEPRECATION")
  private fun micRoute(on: Boolean) {
    routed = on
    try {
      if (on && audio.isBluetoothScoAvailableOffCall) { audio.mode = AudioManager.MODE_IN_COMMUNICATION; audio.startBluetoothSco() }
      else if (!on) { audio.stopBluetoothSco(); audio.mode = AudioManager.MODE_NORMAL }
    } catch (e: Exception) { DriveService.note("micro Bluetooth : $e") }
  }

  @Suppress("DEPRECATION")
  fun focus(pause: Boolean = false) {
    if (closed || (focus != null && (exclusive || !pause))) return
    val kind = if (pause) AudioManager.AUDIOFOCUS_GAIN_TRANSIENT else AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK
    val previous = focus
    val got: Any? = try {
      if (Build.VERSION.SDK_INT >= 26) {
        val r = AudioFocusRequest.Builder(kind)
          .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANCE_NAVIGATION_GUIDANCE)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()).setOnAudioFocusChangeListener {}.build()
        if (audio.requestAudioFocus(r) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) r else null
      } else {
        val l = AudioManager.OnAudioFocusChangeListener {}
        if (audio.requestAudioFocus(l, AudioManager.STREAM_MUSIC, kind) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) l else null
      }
    } catch (e: Exception) { DriveService.note("focus audio : ${e.javaClass.simpleName}"); null }
    if (got == null) return
    focus = got
    exclusive = pause
    if (previous != null) release(previous)
  }

  fun abandonFocus() {
    val f = focus ?: return
    focus = null
    exclusive = false
    release(f)
  }

  @Suppress("DEPRECATION")
  private fun release(f: Any) {
    try {
      if (Build.VERSION.SDK_INT >= 26 && f is AudioFocusRequest) audio.abandonAudioFocusRequest(f)
      else if (f is AudioManager.OnAudioFocusChangeListener) audio.abandonAudioFocus(f)
    } catch (e: Exception) { DriveService.note("focus audio : ${e.javaClass.simpleName}") }
  }

  fun shutdown() {
    if (closed) return
    watchChanged = null
    stopWatch(false)
    closed = true
    ui.removeCallbacksAndMessages(null)
    stopListening()
    if (routed) micRoute(false)
    if (hearing) { hearing = false; Mic.give(true) }
    if (talking) { talking = false; Mic.give(false) }
    done.clear()
    guards.clear()
    isReady = false
    try { tts?.shutdown() } catch (e: Exception) { DriveService.note("synthèse vocale : ${e.javaClass.simpleName}") }
    tts = null
    abandonFocus()
    val had = synchronized(this) { model.also { model = null } }
    if (had != null) thread { VoskShared.release() }
  }
}
