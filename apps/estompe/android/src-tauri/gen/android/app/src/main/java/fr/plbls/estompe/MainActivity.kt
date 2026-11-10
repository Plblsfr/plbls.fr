package fr.plbls.estompe

import android.Manifest
import android.annotation.SuppressLint
import android.app.AppOpsManager
import android.app.PendingIntent
import android.app.PictureInPictureParams
import android.app.RemoteAction
import android.content.BroadcastReceiver
import android.content.Context
import android.content.IntentFilter
import android.content.res.Configuration
import android.graphics.drawable.Icon
import android.util.Rational
import android.bluetooth.BluetoothManager
import android.content.Intent
import android.graphics.Bitmap
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Base64
import android.view.PixelCopy
import android.provider.Settings
import android.speech.tts.TextToSpeech
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.lifecycle.Lifecycle
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.firebase.messaging.FirebaseMessaging
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

/**
 * Pont vers le front (window.EstompeNative) :
 * - fcmToken() : jeton FCM, vide si Firebase n'est pas configuré (google-services.json absent) ;
 * - cancelConv(conv) : retire la notification de la conversation lue sur ce téléphone ;
 * - takeOpenUrl() : adresse (« /#/dm/… ») de la notification qui a lancé l'app (lue une seule fois), vide sinon ;
 * - takeAuthUrl() : redirection OAuth « fr.plbls.estompe:/callback?… » reçue du navigateur (lue une seule fois), vide sinon ;
 * - bgAudioUpdate(json) / bgAudioStop() : service de lecture 2Gether en arrière-plan (PlaybackService, option cachée) ; la page est appelée en retour par window.__estompeBg(toggle|next) ;
 * - voiceUpdate(titre, lien, micro coupé, son coupé) / voiceStop() : service du vocal en cours (VoiceService), page appelée en retour par window.__estompeVoice ;
 * - pipState(json) : état du lecteur 2Gether ({active, square, playing}) pour l'image dans l'image ; le natif répond par window.estompePip(évènement) : toggle, next, close, enter, leave, unavailable ;
 * - installApk(url, sha256) puis installState() : télécharge la mise à jour, vérifie sa somme SHA-256 (ignorée si vide) et ouvre l'installeur Android (« busy » en cours, « done » une fois lancé, sinon le message d'erreur) ; installProgress() : avancement du téléchargement de 0 à 1 (0 si la taille est inconnue).
 * Canal window.EstompeSession (addWebMessageListener, origine https://tauri.localhost et cadre principal seulement, jamais les iframes) :
 * messages JSON { id, op } avec op login, renew, logout, adopt ou screenshot, réponse { id, status, … } ; le refresh token n'en sort jamais.
 */
class EstompeNative(private val activity: MainActivity) {
  @Volatile var token = ""
  @Volatile var authUrl = ""
  @Volatile private var install = ""
  @Volatile private var progress = 0f
  @Volatile var openUrl = ""
  private val session by lazy { Session(activity) }
  @JavascriptInterface fun fcmToken(): String = token
  @JavascriptInterface fun takeOpenUrl(): String = openUrl.also { openUrl = "" }
  @JavascriptInterface fun takeAuthUrl(): String = authUrl.also { authUrl = "" }
  /** Conversation « c:<id> » ou « d:<id> » ouverte dans l'app : retire sa notification. */
  @JavascriptInterface fun cancelConv(conv: String) = dismissRead(activity, conv, null)
  @JavascriptInterface fun pipState(json: String) { val o = JSONObject(json); activity.runOnUiThread { activity.setPip(o.optBoolean("active"), o.optBoolean("square"), o.optBoolean("playing")) } }
  @JavascriptInterface fun installState(): String = install
  @JavascriptInterface fun installProgress(): Float = progress

  fun sessionMessage(json: String): String {
    val m = try { JSONObject(json) } catch (e: JSONException) { return JSONObject().put("status", 400).toString() }
    val r = try {
      when (m.optString("op")) {
        "login" -> session.connecter(m.getString("code"), m.getString("verifier"), m.getString("redirectUri"))
        "renew" -> session.renouveler()
        "logout" -> { session.deconnecter(); JSONObject().put("status", 200) }
        "adopt" -> { session.adopter(m.getString("refresh")); JSONObject().put("status", 200) }
        "screenshot" -> JSONObject().put("status", 200).put("image", screenshot())
        else -> JSONObject().put("status", 400).put("error", "opération inconnue")
      }
    } catch (e: JSONException) {
      JSONObject().put("status", 400).put("error", "requête invalide")
    }
    return r.put("id", m.optInt("id")).toString()
  }

  private fun screenshot(): String {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return ""
    val view = activity.window.decorView
    val bmp = Bitmap.createBitmap(view.width, view.height, Bitmap.Config.ARGB_8888)
    val done = CountDownLatch(1)
    var ok = false
    PixelCopy.request(activity.window, bmp, { ok = it == PixelCopy.SUCCESS; done.countDown() }, Handler(Looper.getMainLooper()))
    if (!done.await(3, TimeUnit.SECONDS) || !ok) return ""
    return ByteArrayOutputStream().also { bmp.compress(Bitmap.CompressFormat.JPEG, 80, it) }.toByteArray().let { Base64.encodeToString(it, Base64.NO_WRAP) }
  }

  @JavascriptInterface fun bgAudioUpdate(json: String) = PlaybackService.update(activity, json)
  @JavascriptInterface fun bgAudioStop() = PlaybackService.stop(activity)
  @JavascriptInterface fun voiceUpdate(title: String, href: String, muted: Boolean, deafened: Boolean) = VoiceService.update(activity, title, href, muted, deafened)
  @JavascriptInterface fun voiceStop() = VoiceService.stop(activity)

  // Mode conduite (DriveService) : réglages en JSON, démarrage/arrêt, moteurs de voix, voitures Bluetooth, modèle de dictée
  private val drive by lazy { DriveStore(activity) }
  @JavascriptInterface fun driveSettings(): String = drive.toJson().toString()
  @JavascriptInterface fun saveDriveSettings(json: String) { try { drive.save(JSONObject(json)) } catch (e: JSONException) { DriveService.note("réglages refusés : $e") } }
  @JavascriptInterface fun driveActive(): Boolean = DriveService.running
  @JavascriptInterface fun driveStop() = DriveService.stop(activity)
  @JavascriptInterface fun driveLog(): String = DriveService.log()
  @JavascriptInterface fun driveDownloadModel() = VoskModel.download(activity)
  /** « ready » (prêt), « busy » (téléchargement), un message d'erreur, ou vide (à télécharger). */
  @JavascriptInterface fun driveModel(): String = if (VoskModel.state.isNotEmpty()) VoskModel.state else if (VoskModel.ready(activity)) "ready" else ""

  /** Vide si le mode démarre ; sinon la raison (permission demandée, à réaccorder puis réessayer). */
  @JavascriptInterface fun driveStart(): String {
    val missing = buildList {
      add(Manifest.permission.RECORD_AUDIO)
      if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
      if (Build.VERSION.SDK_INT >= 31 && drive.load().autoBluetooth) add(Manifest.permission.BLUETOOTH_CONNECT)
    }.filter { activity.checkSelfPermission(it) != PackageManager.PERMISSION_GRANTED }
    if (missing.isNotEmpty()) {
      activity.runOnUiThread { activity.requestPermissions(missing.toTypedArray(), 1) }
      return "Autorise le micro et les notifications, puis réessaie."
    }
    DriveService.start(activity)
    return ""
  }

  @JavascriptInterface fun driveEngines(): String {
    val pm = activity.packageManager
    return JSONArray(pm.queryIntentServices(Intent(TextToSpeech.Engine.INTENT_ACTION_TTS_SERVICE), 0).map {
      JSONObject().put("id", it.serviceInfo.packageName).put("label", it.loadLabel(pm).toString())
    }).toString()
  }

  @SuppressLint("MissingPermission")
  @JavascriptInterface fun driveBluetooth(): String {
    if (Build.VERSION.SDK_INT >= 31 && activity.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) != PackageManager.PERMISSION_GRANTED) {
      activity.runOnUiThread { activity.requestPermissions(arrayOf(Manifest.permission.BLUETOOTH_CONNECT), 2) }
      return "[]"
    }
    val bonded = activity.getSystemService(BluetoothManager::class.java)?.adapter?.bondedDevices.orEmpty()
    return JSONArray(bonded.map { JSONObject().put("id", it.address).put("label", it.name ?: it.address) }).toString()
  }

  @JavascriptInterface fun installApk(url: String, sha256: String) {
    if (install == "busy") return
    // seule l'adresse des mises à jour est acceptée ; Android refuse de toute façon un APK signé avec une autre clé
    if (!url.startsWith("https://estompe.plbls.fr/updates/")) { install = "adresse de mise à jour refusée"; return }
    if (!activity.packageManager.canRequestPackageInstalls()) {
      install = "Autorise Estompe à installer des applications dans les réglages qui viennent de s'ouvrir, puis réessaie."
      activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
      return
    }
    install = "busy"
    progress = 0f
    thread {
      try {
        val apk = File(activity.cacheDir, "Estompe.apk")
        val digest = MessageDigest.getInstance("SHA-256")
        val conn = URL(url).openConnection()
        val total = conn.contentLengthLong
        var done = 0L
        conn.getInputStream().use { i -> apk.outputStream().use { o ->
          val buf = ByteArray(64 * 1024)
          while (true) {
            val n = i.read(buf)
            if (n < 0) break
            o.write(buf, 0, n)
            digest.update(buf, 0, n)
            done += n
            if (total > 0) progress = done.toFloat() / total
          }
        } }
        val sum = digest.digest().joinToString("") { "%02x".format(it) }
        if (sha256.isNotEmpty() && !sum.equals(sha256, ignoreCase = true)) {
          apk.delete()
          install = "somme de contrôle de la mise à jour incorrecte : téléchargement refusé"
          return@thread
        }
        val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", apk)
        activity.startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive")
          .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK))
        install = "done"
      } catch (e: Exception) {
        install = "téléchargement impossible : ${e.message}"
      }
    }
  }
}

class MainActivity : TauriActivity() {
  private val native = EstompeNative(this)
  private var webView: WebView? = null
  private var pipOn = false
  private var pipSquare = false
  private var pipPlaying = false
  private val pipReceiver = object : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
      val evt = intent.action?.removePrefix(PIP_PREFIX) ?: return
      pip(evt)
      if (evt == "close") moveTaskToBack(true)
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // la WebView ne s'étend pas sous les barres système (ni sous le clavier) : sinon l'en-tête, burger compris, passe sous la barre d'état (EST-22)
    ViewCompat.setOnApplyWindowInsetsListener(findViewById(android.R.id.content)) { v, insets ->
      val i = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout() or WindowInsetsCompat.Type.ime())
      v.setPadding(i.left, i.top, i.right, i.bottom)
      WindowInsetsCompat.CONSUMED
    }
    ContextCompat.registerReceiver(this, pipReceiver, IntentFilter().apply { PIP_EVENTS.forEach { addAction(PIP_PREFIX + it) } }, ContextCompat.RECEIVER_NOT_EXPORTED)
    keepAuthUrl(intent)
    keepOpenUrl(intent)
    ensureChannels(this)
    try {
      FirebaseMessaging.getInstance().token.addOnSuccessListener { native.token = it; Session(this).fcmToken = it }
    } catch (e: IllegalStateException) {
      // FirebaseApp non initialisé : pas de google-services.json dans ce build
    }
  }

  // singleTask : le retour du navigateur arrive ici, l'app (suspendue pendant la connexion) n'a pas de serveur local à interroger
  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    keepAuthUrl(intent)
    // app déjà lancée : la page change de conversation sans recharger
    keepOpenUrl(intent)
    if (native.openUrl.isNotEmpty()) webView?.evaluateJavascript("location.hash = ${JSONObject.quote(native.takeOpenUrl().removePrefix("/"))}", null)
  }

  // seules les adresses internes « /#/… » (envoyées par notre serveur) sont acceptées
  private fun keepOpenUrl(intent: Intent?) {
    intent?.getStringExtra("open")?.takeIf { it.startsWith("/#/") }?.let { native.openUrl = it }
  }

  private fun keepAuthUrl(intent: Intent?) {
    val data = intent?.data ?: return
    if (data.scheme == "fr.plbls.estompe") native.authUrl = keptAuthUrl(native.authUrl, data.toString())
  }

  override fun onResume() {
    super.onResume()
    PlaybackService.page = { action -> runOnUiThread { webView?.evaluateJavascript("window.__estompeBg && window.__estompeBg('$action')", null) } }
    VoiceService.page = { action -> runOnUiThread { webView?.evaluateJavascript("window.__estompeVoice && window.__estompeVoice('$action')", null) } }
  }

  override fun onPause() {
    super.onPause()
    if (VoiceService.running || PlaybackService.running) keepWebViewAlive()
  }

  override fun onStop() {
    super.onStop()
    if (VoiceService.running || PlaybackService.running) keepWebViewAlive()
  }

  override fun onDestroy() {
    PlaybackService.page = null
    PlaybackService.stop(this)
    unregisterReceiver(pipReceiver)
    super.onDestroy()
  }

  private fun keepWebViewAlive() {
    webView?.onResume()
    webView?.resumeTimers()
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    webView.addJavascriptInterface(native, "EstompeNative")
    if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
      WebViewCompat.addWebMessageListener(webView, "EstompeSession", setOf(ORIGINE)) { view, message, _, isMainFrame, reply ->
        val data = message.data
        if (isMainFrame && data != null) thread { val r = native.sessionMessage(data); view.post { reply.postMessage(r) } }
      }
    }
  }

  private fun pip(evt: String) = webView?.evaluateJavascript("window.estompePip?.(${JSONObject.quote(evt)})", null)

  private fun pipAvailable(): Boolean {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O || !packageManager.hasSystemFeature(PackageManager.FEATURE_PICTURE_IN_PICTURE)) return false
    val ops = getSystemService(AppOpsManager::class.java)
    return ops.checkOpNoThrow(AppOpsManager.OPSTR_PICTURE_IN_PICTURE, android.os.Process.myUid(), packageName) == AppOpsManager.MODE_ALLOWED
  }

  private fun pipAction(evt: String, icon: Int, title: String): RemoteAction {
    val intent = PendingIntent.getBroadcast(this, evt.hashCode(), Intent(PIP_PREFIX + evt).setPackage(packageName), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    return RemoteAction(Icon.createWithResource(this, icon), title, title, intent)
  }

  private fun pipParams(): PictureInPictureParams {
    val ratio = if (pipSquare) Rational(1, 1) else Rational(16, 9)
    val actions = listOf(
      if (pipPlaying) pipAction("toggle", android.R.drawable.ic_media_pause, "Pause") else pipAction("toggle", android.R.drawable.ic_media_play, "Lecture"),
      pipAction("next", android.R.drawable.ic_media_next, "Suivant"),
      pipAction("close", android.R.drawable.ic_menu_close_clear_cancel, "Fermer"),
    )
    return PictureInPictureParams.Builder().setAspectRatio(ratio).setActions(actions).apply {
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) setAutoEnterEnabled(pipOn && pipAvailable())
    }.build()
  }

  fun setPip(on: Boolean, square: Boolean, playing: Boolean) {
    pipOn = on
    pipSquare = square
    pipPlaying = playing
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) setPictureInPictureParams(pipParams())
  }

  override fun onUserLeaveHint() {
    super.onUserLeaveHint()
    if (!pipOn) return
    if (!pipAvailable()) pip("unavailable")
    else if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
      try { enterPictureInPictureMode(pipParams()) } catch (e: IllegalStateException) { pip("unavailable") }
    }
  }

  override fun onPictureInPictureModeChanged(isInPictureInPictureMode: Boolean, newConfig: Configuration) {
    super.onPictureInPictureModeChanged(isInPictureInPictureMode, newConfig)
    if (isInPictureInPictureMode) {
      webView?.onResume()
      webView?.resumeTimers()
      pip("enter")
    } else {
      pip("leave")
      if (lifecycle.currentState == Lifecycle.State.CREATED) pip("close")
    }
  }

  companion object {
    private const val ORIGINE = "https://tauri.localhost"
    private const val PIP_PREFIX = "fr.plbls.estompe.PIP_"
    private val PIP_EVENTS = listOf("toggle", "next", "close")
  }
}
