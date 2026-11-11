package fr.plbls.estompe

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
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
  private var talking = 0
  internal var watcher: Voice? = null
  var freed: (() -> Unit)? = null
  val busy get() = hearing || talking > 0

  internal fun take(listen: Boolean) {
    if (listen) hearing = true else talking++
    watcher?.preempt()
  }

  internal fun give(listen: Boolean) {
    if (listen) hearing = false else if (talking > 0) talking--
    if (!busy) freed?.invoke()
  }
}

class Voice(private val context: Context, private val settings: DriveSettings, private val ready: () -> Unit, private val noEngine: (String) -> Unit) {
  private val ui = Handler(Looper.getMainLooper())
  private val audio = context.getSystemService(AudioManager::class.java)
  private val done = HashMap<String, () -> Unit>()
  private var tts: TextToSpeech? = null
  private var utterance = 0
  private var talking = false
  private var hearing = false
  private var routed = false
  @Volatile private var closed = false
  private var focus: Any? = null
  private var model: Model? = null
  private var speech: SpeechService? = null
  private var recognizer: Recognizer? = null
  private var wakeSpeech: SpeechService? = null
  private var wakeRecognizer: Recognizer? = null
  private var watchWanted = false
  private var watchToken = 0
  private var watchChanged: ((Boolean, Boolean) -> Unit)? = null

  var isReady = false
    private set
  val watching get() = wakeSpeech != null

  init {
    tts = try {
      TextToSpeech(context, { code -> onInit(code) }, settings.engine.ifEmpty { null })
    } catch (e: Exception) {
      DriveService.note("synthèse vocale : ${e.javaClass.simpleName}")
      null
    }
    if (tts == null) ui.post { if (!closed) noEngine("Aucun moteur de synthèse vocale : touche pour en installer un.") }
  }

  private fun onInit(code: Int) {
    if (closed) return
    val t = tts
    if (code != TextToSpeech.SUCCESS || t == null) return noEngine("Aucun moteur de synthèse vocale : touche pour en installer un.")
    if (t.setLanguage(Locale.FRENCH) < TextToSpeech.LANG_AVAILABLE) return noEngine("Voix française absente : touche pour l'installer.")
    t.setSpeechRate(settings.safeRate)
    t.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANCE_NAVIGATION_GUIDANCE)
      .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
    t.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
      override fun onStart(id: String?) {}
      override fun onDone(id: String?) { ui.post { id?.let { finished(it) } } }
      @Deprecated("API < 21") override fun onError(id: String?) { ui.post { id?.let { finished(it) } } }
      override fun onError(id: String?, code: Int) { DriveService.note("synthèse vocale : erreur $code"); onError(id) }
    })
    isReady = true
    ready()
  }

  fun say(text: String, then: () -> Unit = {}) {
    val t = tts
    if (t == null || !isReady || closed) return then()
    val id = "u${utterance++}"
    done[id] = then
    if (!talking) { talking = true; Mic.take(false) }
    focus()
    if (t.speak(text, TextToSpeech.QUEUE_ADD, null, id) != TextToSpeech.SUCCESS) { DriveService.note("synthèse vocale : refus de lire"); finished(id) }
  }

  private fun finished(id: String) {
    if (closed) return
    val then = done.remove(id) ?: return
    try { then() } catch (e: Exception) { DriveService.note("synthèse vocale : rappel ${e.javaClass.simpleName}") }
    if (done.isEmpty() && talking && !closed) { talking = false; Mic.give(false) }
  }

  fun listen(ms: Int, bluetooth: Boolean = true, result: (String?) -> Unit) {
    if (closed) return
    if (Mic.hearing) {
      DriveService.note("micro occupé par une autre écoute")
      ui.post { if (!closed) result(null) }
      return
    }
    hearing = true
    Mic.take(true)
    thread {
      val loaded = loadModel()
      ui.post {
        if (closed) return@post
        if (loaded == null) { hearing = false; Mic.give(true); return@post result(null) }
        if (bluetooth) micRoute(true)
        var over = false
        fun end(text: String?) {
          if (over || closed) return
          over = true
          stopListening()
          if (bluetooth) micRoute(false)
          hearing = false
          Mic.give(true)
          try { result(text) } catch (e: Exception) { DriveService.note("reconnaissance vocale : rappel ${e.javaClass.simpleName}") }
        }
        ui.postDelayed({
          if (closed) return@postDelayed
          try {
            val r = Recognizer(loaded, 16000f)
            recognizer = r
            val s = SpeechService(r, 16000f)
            speech = s
            s.startListening(object : RecognitionListener {
              override fun onPartialResult(hypothesis: String?) {}
              override fun onResult(hypothesis: String?) { end(text(hypothesis)) }
              override fun onFinalResult(hypothesis: String?) { end(text(hypothesis)) }
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
        try {
          val r = Recognizer(loaded, 16000f)
          wakeRecognizer = r
          val s = SpeechService(r, 16000f)
          wakeSpeech = s
          s.startListening(object : RecognitionListener {
            override fun onPartialResult(hypothesis: String?) {}
            override fun onResult(hypothesis: String?) { if (token == watchToken) text(hypothesis)?.let(heard) }
            override fun onFinalResult(hypothesis: String?) {}
            override fun onError(e: Exception?) {
              if (token != watchToken) return
              DriveService.note("veille du mot d'éveil : ${e?.javaClass?.simpleName}")
              stopWatch(true)
            }
            override fun onTimeout() {}
          })
          changed(true, false)
        } catch (e: Exception) {
          DriveService.note("micro de veille : ${e.javaClass.simpleName}")
          stopWatch(true)
        }
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
    val s = wakeSpeech
    val r = wakeRecognizer
    wakeSpeech = null
    wakeRecognizer = null
    close(s, r)
    val c = watchChanged
    watchChanged = null
    c?.invoke(false, failed)
  }

  private fun text(h: String?) = try { h?.let { JSONObject(it).optString("text") }?.takeIf { it.isNotBlank() } } catch (e: Exception) { null }

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
    close(s, r)
  }

  private fun close(s: SpeechService?, r: Recognizer?) {
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
  fun focus() {
    if (focus != null || closed) return
    if (Build.VERSION.SDK_INT >= 26) {
      val r = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
        .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ASSISTANCE_NAVIGATION_GUIDANCE)
          .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build()).setOnAudioFocusChangeListener {}.build()
      if (audio.requestAudioFocus(r) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) focus = r
    } else {
      val l = AudioManager.OnAudioFocusChangeListener {}
      if (audio.requestAudioFocus(l, AudioManager.STREAM_MUSIC, AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) focus = l
    }
  }

  @Suppress("DEPRECATION")
  fun abandonFocus() {
    val f = focus ?: return
    focus = null
    if (Build.VERSION.SDK_INT >= 26 && f is AudioFocusRequest) audio.abandonAudioFocusRequest(f)
    else if (f is AudioManager.OnAudioFocusChangeListener) audio.abandonAudioFocus(f)
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
    try { tts?.shutdown() } catch (e: Exception) { DriveService.note("synthèse vocale : ${e.javaClass.simpleName}") }
    tts = null
    abandonFocus()
    val had = synchronized(this) { model.also { model = null } }
    if (had != null) thread { VoskShared.release() }
  }
}
