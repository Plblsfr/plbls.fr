package fr.plbls.estompe

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.drawable.Icon
import android.media.MediaMetadata
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Build
import android.os.IBinder
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject
import java.net.URL
import kotlin.concurrent.thread

class PlaybackService : Service() {
  private var session: MediaSession? = null
  private var title = ""
  private var artist = ""
  private var cover: Bitmap? = null
  private var coverUrl = ""
  private var playing = false
  private var stalled = false

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onCreate() {
    super.onCreate()
    getSystemService(NotificationManager::class.java).createNotificationChannel(NotificationChannel(CHANNEL, "Lecture 2Gether", NotificationManager.IMPORTANCE_LOW))
    session = MediaSession(this, "estompe").apply {
      setCallback(object : MediaSession.Callback() {
        override fun onPlay() = notifyPage("toggle")
        override fun onPause() = notifyPage("toggle")
        override fun onSkipToNext() = notifyPage("next")
      })
      isActive = true
    }
    running = true
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    intent?.getStringExtra("state")?.let { raw ->
      val o = JSONObject(raw)
      title = o.optString("title")
      artist = o.optString("artist")
      playing = o.optBoolean("playing")
      stalled = o.optBoolean("stalled")
      loadCover(o.optString("cover"))
    }
    refresh()
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    running = false
    session?.release()
    session = null
    super.onDestroy()
  }

  private fun loadCover(url: String) {
    if (url == coverUrl) return
    coverUrl = url
    cover = null
    val api = Session(this).apiUrl
    if (url.isEmpty() || api.isNullOrEmpty() || !url.startsWith(api)) return
    thread {
      val bmp = try { URL(url).openStream().use { BitmapFactory.decodeStream(it) } } catch (e: Exception) { null }
      if (url == coverUrl && bmp != null) { cover = bmp; refresh() }
    }
  }

  private fun refresh() {
    val s = session ?: return
    s.setMetadata(MediaMetadata.Builder().putString(MediaMetadata.METADATA_KEY_TITLE, title).putString(MediaMetadata.METADATA_KEY_ARTIST, artist)
      .putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, cover).build())
    val state = if (stalled) PlaybackState.STATE_ERROR else if (playing) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED
    s.setPlaybackState(PlaybackState.Builder().setActions(PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_PLAY_PAUSE or PlaybackState.ACTION_SKIP_TO_NEXT)
      .setState(state, PlaybackState.PLAYBACK_POSITION_UNKNOWN, 1f).build())
    val type = if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else 0
    try {
      ServiceCompat.startForeground(this, NOTIF_ID, notification(s), type)
    } catch (e: Exception) {
      DriveService.note("service de lecture au premier plan : $e")
      stopSelf()
    }
  }

  private fun notification(s: MediaSession): Notification {
    fun action(name: String, code: Int, icon: Int, label: String) = Notification.Action.Builder(Icon.createWithResource(this, icon), label,
      PendingIntent.getBroadcast(this, code, Intent(this, PlaybackActionReceiver::class.java).setAction(name), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)).build()
    val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
    val toggle = if (playing && !stalled) action(ACTION_TOGGLE, 1, android.R.drawable.ic_media_pause, "Pause") else action(ACTION_TOGGLE, 1, android.R.drawable.ic_media_play, if (stalled) "Reprendre" else "Lecture")
    return Notification.Builder(this, CHANNEL).setSmallIcon(R.drawable.ic_notification).setColor(getColor(R.color.notification_accent))
      .setContentTitle(title).setContentText(if (stalled) "Lecture interrompée par le lecteur (page non visible)" else artist)
      .setLargeIcon(cover).setOngoing(true).setOnlyAlertOnce(true).setContentIntent(open).setCategory(Notification.CATEGORY_TRANSPORT)
      .addAction(toggle).addAction(action(ACTION_NEXT, 2, android.R.drawable.ic_media_next, "Suivant"))
      .setStyle(Notification.MediaStyle().setMediaSession(s.sessionToken).setShowActionsInCompactView(0, 1)).build()
  }

  companion object {
    const val ACTION_TOGGLE = "fr.plbls.estompe.playback.TOGGLE"
    const val ACTION_NEXT = "fr.plbls.estompe.playback.NEXT"
    private const val CHANNEL = "playback"
    private const val NOTIF_ID = 4244

    @Volatile var running = false
    @Volatile var page: ((String) -> Unit)? = null

    fun notifyPage(action: String) { page?.invoke(action) }

    fun update(c: Context, json: String) {
      try {
        val i = Intent(c, PlaybackService::class.java).putExtra("state", json)
        if (running) c.startService(i) else ContextCompat.startForegroundService(c, i)
      } catch (e: Exception) {
        DriveService.note("démarrage du service de lecture : $e")
      }
    }

    fun stop(c: Context) { if (running) c.stopService(Intent(c, PlaybackService::class.java)) }
  }
}

class PlaybackActionReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    PlaybackService.notifyPage(when (intent.action) {
      PlaybackService.ACTION_TOGGLE -> "toggle"
      PlaybackService.ACTION_NEXT -> "next"
      else -> return
    })
  }
}
