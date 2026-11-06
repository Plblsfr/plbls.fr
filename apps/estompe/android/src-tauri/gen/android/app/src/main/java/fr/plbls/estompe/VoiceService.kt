package fr.plbls.estompe

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat

/** Vocal en cours : service au premier plan (micro), wake lock, focus audio de communication et notification de contrôle. */
class VoiceService : Service() {
  private lateinit var audio: AudioManager
  private var wake: PowerManager.WakeLock? = null
  private var focus: AudioFocusRequest? = null
  private var title = ""
  private var href = "/#/"
  private var muted = false
  private var deafened = false

  private val routeCallback = object : android.media.AudioDeviceCallback() {
    override fun onAudioDevicesAdded(added: Array<out AudioDeviceInfo>?) = route()
    override fun onAudioDevicesRemoved(removed: Array<out AudioDeviceInfo>?) = route()
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    audio = getSystemService(AudioManager::class.java)
    val m = getSystemService(NotificationManager::class.java)
    m.createNotificationChannel(NotificationChannel(CHANNEL, "Vocal en cours", NotificationManager.IMPORTANCE_LOW))
    wake = getSystemService(PowerManager::class.java).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "estompe:vocal").apply { acquire() }
    audio.mode = AudioManager.MODE_IN_COMMUNICATION
    requestFocus()
    audio.registerAudioDeviceCallback(routeCallback, null)
    route()
    running = true
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    intent?.let {
      title = it.getStringExtra("title") ?: title
      href = it.getStringExtra("href")?.takeIf { h -> h.startsWith("/#/") } ?: href
      muted = it.getBooleanExtra("muted", muted)
      deafened = it.getBooleanExtra("deafened", deafened)
    }
    var type = 0
    if (Build.VERSION.SDK_INT >= 30) type = ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
    try {
      ServiceCompat.startForeground(this, NOTIF_ID, notification(), type)
    } catch (e: Exception) {
      DriveService.note("service vocal au premier plan : $e")
      stopSelf()
    }
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    running = false
    audio.unregisterAudioDeviceCallback(routeCallback)
    abandonFocus()
    audio.mode = AudioManager.MODE_NORMAL
    if (Build.VERSION.SDK_INT >= 31) audio.clearCommunicationDevice() else speaker(false)
    wake?.takeIf { it.isHeld }?.release()
    super.onDestroy()
  }

  private fun notification(): Notification {
    fun action(name: String, code: Int) = PendingIntent.getBroadcast(this, code,
      Intent(this, VoiceActionReceiver::class.java).setAction(name), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).putExtra("open", href)
      .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    return NotificationCompat.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_notification).setColor(getColor(R.color.notification_accent))
      .setContentTitle("En vocal dans $title").setOngoing(true).setOnlyAlertOnce(true).setContentIntent(open)
      .setCategory(NotificationCompat.CATEGORY_CALL)
      .addAction(0, if (muted && !deafened) "Réactiver le micro" else "Couper le micro", action(ACTION_MUTE, 1))
      .addAction(0, if (deafened) "Réactiver le son" else "Couper le son", action(ACTION_DEAFEN, 2))
      .addAction(0, "Raccrocher", action(ACTION_LEAVE, 3)).build()
  }

  private fun requestFocus() {
    val r = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
      .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
        .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
      .setOnAudioFocusChangeListener { change ->
        when (change) {
          AudioManager.AUDIOFOCUS_LOSS, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> notifyPage("pause")
          AudioManager.AUDIOFOCUS_GAIN -> notifyPage("resume")
        }
      }.build()
    if (audio.requestAudioFocus(r) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) focus = r
  }

  private fun abandonFocus() {
    focus?.let { audio.abandonAudioFocusRequest(it) }
    focus = null
  }

  private fun route() {
    val outs = audio.getDevices(AudioManager.GET_DEVICES_OUTPUTS)
    val headset = outs.firstOrNull { it.type in HEADSETS }
    if (Build.VERSION.SDK_INT >= 31) {
      val target = audio.availableCommunicationDevices.firstOrNull { it.type in HEADSETS }
        ?: audio.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
      if (target != null) audio.setCommunicationDevice(target)
    } else {
      speaker(headset == null)
    }
  }

  @Suppress("DEPRECATION")
  private fun speaker(on: Boolean) { audio.isSpeakerphoneOn = on }

  companion object {
    const val ACTION_MUTE = "fr.plbls.estompe.voice.MUTE"
    const val ACTION_DEAFEN = "fr.plbls.estompe.voice.DEAFEN"
    const val ACTION_LEAVE = "fr.plbls.estompe.voice.LEAVE"
    private const val CHANNEL = "call"
    private const val NOTIF_ID = 4243
    private val HEADSETS = setOf(AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
      AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
      AudioDeviceInfo.TYPE_BLE_HEADSET)

    @Volatile var running = false
    @Volatile var page: ((String) -> Unit)? = null

    fun notifyPage(action: String) { page?.invoke(action) }

    fun update(c: Context, title: String, href: String, muted: Boolean, deafened: Boolean) {
      try {
        val i = Intent(c, VoiceService::class.java).putExtra("title", title).putExtra("href", href).putExtra("muted", muted).putExtra("deafened", deafened)
        if (running) c.startService(i) else ContextCompat.startForegroundService(c, i)
      } catch (e: Exception) {
        DriveService.note("démarrage du service vocal : $e")
      }
    }

    fun stop(c: Context) { c.stopService(Intent(c, VoiceService::class.java)) }
  }
}

/** Boutons de la notification : l'action est exécutée par la page, qui détient l'état du vocal. */
class VoiceActionReceiver : android.content.BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    VoiceService.notifyPage(when (intent.action) {
      VoiceService.ACTION_MUTE -> "mute"
      VoiceService.ACTION_DEAFEN -> "deafen"
      VoiceService.ACTION_LEAVE -> "leave"
      else -> return
    })
  }
}
