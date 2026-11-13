package fr.plbls.estompe

import android.app.ActivityManager
import android.app.SearchManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.media.AudioManager
import android.media.browse.MediaBrowser
import android.media.session.MediaController
import android.media.session.MediaSessionManager
import android.media.session.PlaybackState
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.MediaStore
import android.provider.Settings
import android.service.media.MediaBrowserService
import android.view.KeyEvent
import java.text.Collator
import java.util.Locale

fun appInForeground(): Boolean = try {
  val info = ActivityManager.RunningAppProcessInfo()
  ActivityManager.getMyMemoryState(info)
  info.importance == ActivityManager.RunningAppProcessInfo.IMPORTANCE_FOREGROUND
} catch (e: Exception) { false }

class MusicControl(private val context: Context) {
  private val app = context.applicationContext
  private val ui = Handler(Looper.getMainLooper())

  @Suppress("DEPRECATION")
  fun apps(): List<MusicApp> = try {
    val pm = app.packageManager
    val collator = Collator.getInstance(Locale.FRENCH)
    pm.queryIntentServices(Intent(MediaBrowserService.SERVICE_INTERFACE), 0).mapNotNull { it.serviceInfo }
      .filter { it.packageName != app.packageName }.distinctBy { it.packageName }
      .map { s -> MusicApp(s.packageName, try { s.applicationInfo.loadLabel(pm).toString() } catch (e: Exception) { s.packageName }) }
      .sortedWith(Comparator { a, b -> collator.compare(a.label, b.label) })
  } catch (e: Exception) {
    DriveService.note("apps de musique : $e")
    emptyList()
  }

  fun transport(action: Transport, app: String, fallback: String = ""): Boolean = try {
    when (action) {
      Transport.VOLUME_UP -> volume(AudioManager.ADJUST_RAISE)
      Transport.VOLUME_DOWN -> volume(AudioManager.ADJUST_LOWER)
      Transport.PLAY -> controller(app)?.let { it.transportControls.play(); true } ?: start(app.ifEmpty { fallback })
      Transport.PAUSE -> controller(app)?.let { it.transportControls.pause(); true } ?: false
      Transport.NEXT -> controller(app)?.let { it.transportControls.skipToNext(); true } ?: false
      Transport.PREVIOUS -> controller(app)?.let { it.transportControls.skipToPrevious(); true } ?: false
    }
  } catch (e: Exception) {
    DriveService.note("musique : commande ${action.name.lowercase()} impossible ($e)")
    false
  }

  fun search(query: String, app: String, done: (Boolean) -> Unit) {
    ui.post { find(query, app, Bundle().apply { putString(MediaStore.EXTRA_MEDIA_FOCUS, "vnd.android.cursor.item/*") }, once(done)) }
  }

  fun playlist(name: String, app: String, done: (Boolean) -> Unit) {
    ui.post { browse(name, app, once(done)) }
  }

  fun canLaunch(): Boolean = appInForeground() || try { Settings.canDrawOverlays(app) } catch (e: Exception) { false }

  private fun start(pkg: String): Boolean {
    val keyed = playKey(pkg)
    if (keyed && AssistantListener.enabled(app)) return true
    return open(pkg) || keyed
  }

  private fun open(pkg: String): Boolean {
    if (pkg.isEmpty()) return false
    if (!canLaunch()) { DriveService.note("musique : ouverture de l'app impossible en arrière-plan"); return false }
    val intent = try { app.packageManager.getLaunchIntentForPackage(pkg) } catch (e: Exception) { null }
    if (intent == null) { DriveService.note("musique : app sans écran d'accueil"); return false }
    try { app.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } catch (e: Exception) {
      DriveService.note("musique : ouverture de l'app refusée ($e)")
      return false
    }
    val until = SystemClock.elapsedRealtime() + OPEN_WAIT_MS
    while (Looper.myLooper() != Looper.getMainLooper() && SystemClock.elapsedRealtime() < until) {
      val c = sessions().firstOrNull { it.packageName == pkg }
      if (c != null) {
        try { c.transportControls.play() } catch (e: Exception) { DriveService.note("musique : lecture refusée ($e)") }
        break
      }
      try { Thread.sleep(PLAY_POLL_MS) } catch (e: InterruptedException) { Thread.currentThread().interrupt(); break }
    }
    return true
  }

  private fun once(done: (Boolean) -> Unit): (Boolean) -> Unit {
    var called = false
    return { ok ->
      val run = { if (!called) { called = true; try { done(ok) } catch (e: Exception) { DriveService.note("musique : rappel ${e.javaClass.simpleName}") } } }
      if (Looper.myLooper() == Looper.getMainLooper()) run() else ui.post { run() }
    }
  }

  private fun sessions(): List<MediaController> = try {
    app.getSystemService(MediaSessionManager::class.java)?.getActiveSessions(AssistantListener.component(app)).orEmpty()
      .filter { it.packageName != app.packageName }
  } catch (e: SecurityException) {
    DriveService.note("musique : accès aux notifications requis pour piloter les sessions média")
    emptyList()
  } catch (e: Exception) {
    DriveService.note("musique : sessions média illisibles ($e)")
    emptyList()
  }

  private fun controller(pkg: String): MediaController? {
    val all = sessions()
    if (pkg.isNotEmpty()) return all.firstOrNull { it.packageName == pkg }
    return all.firstOrNull { it.playbackState?.state == PlaybackState.STATE_PLAYING } ?: all.firstOrNull()
  }

  private fun target(pkg: String): String = pkg.ifEmpty { controller("")?.packageName ?: apps().firstOrNull()?.id.orEmpty() }

  private fun volume(direction: Int): Boolean {
    val audio = app.getSystemService(AudioManager::class.java) ?: return false
    audio.adjustStreamVolume(AudioManager.STREAM_MUSIC, direction, AudioManager.FLAG_SHOW_UI)
    return true
  }

  private fun playKey(pkg: String): Boolean {
    val down = KeyEvent(KeyEvent.ACTION_DOWN, KeyEvent.KEYCODE_MEDIA_PLAY)
    val up = KeyEvent(KeyEvent.ACTION_UP, KeyEvent.KEYCODE_MEDIA_PLAY)
    var handed = false
    if (pkg.isNotEmpty()) {
      val receiver = try {
        @Suppress("DEPRECATION")
        app.packageManager.queryBroadcastReceivers(Intent(Intent.ACTION_MEDIA_BUTTON).setPackage(pkg), 0).firstOrNull()?.activityInfo
      } catch (e: Exception) { null }
      if (receiver != null) {
        val to = ComponentName(receiver.packageName, receiver.name)
        app.sendBroadcast(Intent(Intent.ACTION_MEDIA_BUTTON).setComponent(to).putExtra(Intent.EXTRA_KEY_EVENT, down))
        app.sendBroadcast(Intent(Intent.ACTION_MEDIA_BUTTON).setComponent(to).putExtra(Intent.EXTRA_KEY_EVENT, up))
        handed = true
      }
    }
    if (!handed) {
      val audio = app.getSystemService(AudioManager::class.java) ?: return false
      audio.dispatchMediaKeyEvent(down)
      audio.dispatchMediaKeyEvent(up)
    }
    if (!AssistantListener.enabled(app)) return handed
    return playing(pkg)
  }

  private fun playing(pkg: String): Boolean {
    val until = SystemClock.elapsedRealtime() + PLAY_CHECK_MS
    while (true) {
      val found = sessions().any { (pkg.isEmpty() || it.packageName == pkg) && it.playbackState?.state.let { s -> s == PlaybackState.STATE_PLAYING || s == PlaybackState.STATE_BUFFERING } }
      if (found) return true
      if (SystemClock.elapsedRealtime() >= until || Looper.myLooper() == Looper.getMainLooper()) return false
      try { Thread.sleep(PLAY_POLL_MS) } catch (e: InterruptedException) { Thread.currentThread().interrupt(); return false }
    }
  }

  private fun find(query: String, pkg: String, extras: Bundle, done: (Boolean) -> Unit) {
    try {
      val to = target(pkg)
      if (to.isEmpty() || query.isBlank()) { DriveService.note("musique : aucune app pour la recherche"); return done(false) }
      val c = controller(to)
      if (c != null) {
        c.transportControls.playFromSearch(query, extras)
        return done(true)
      }
      val last = { if (canLaunch() && startSearch(query, to, extras)) done(true) else { DriveService.note("musique : recherche impossible en arrière-plan"); done(false) } }
      val started = connect(to, { ok -> if (ok) done(true) else last() }) { browser, finish ->
        try {
          MediaController(app, browser.sessionToken).transportControls.playFromSearch(query, extras)
          finish(true)
        } catch (e: Exception) {
          DriveService.note("musique : recherche refusée par le catalogue ($e)")
          finish(false)
        }
      }
      if (!started) last()
    } catch (e: Exception) {
      DriveService.note("musique : recherche impossible ($e)")
      done(false)
    }
  }

  private fun startSearch(query: String, pkg: String, extras: Bundle): Boolean = try {
    val intent = Intent(MediaStore.INTENT_ACTION_MEDIA_PLAY_FROM_SEARCH).setPackage(pkg).putExtras(extras)
      .putExtra(SearchManager.QUERY, query).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    if (intent.resolveActivity(app.packageManager) == null) false else { app.startActivity(intent); true }
  } catch (e: Exception) {
    DriveService.note("musique : lancement de la recherche refusé ($e)")
    false
  }

  private fun browserComponent(pkg: String): ComponentName? = try {
    @Suppress("DEPRECATION")
    app.packageManager.queryIntentServices(Intent(MediaBrowserService.SERVICE_INTERFACE).setPackage(pkg), 0)
      .firstOrNull()?.serviceInfo?.let { ComponentName(it.packageName, it.name) }
  } catch (e: Exception) { null }

  private fun connect(pkg: String, result: (Boolean) -> Unit, connected: (MediaBrowser, (Boolean) -> Unit) -> Unit): Boolean {
    val component = browserComponent(pkg) ?: return false
    var browser: MediaBrowser? = null
    var over = false
    lateinit var guard: Runnable
    fun close(later: Boolean) {
      ui.removeCallbacks(guard)
      val b = browser ?: return
      browser = null
      val shut = Runnable { try { b.disconnect() } catch (e: Exception) { DriveService.note("musique : déconnexion du catalogue ($e)") } }
      if (later) ui.postDelayed(shut, LINGER_MS) else shut.run()
    }
    val finish: (Boolean) -> Unit = { ok ->
      if (!over) {
        over = true
        close(ok)
        try { result(ok) } catch (e: Exception) { DriveService.note("musique : rappel ${e.javaClass.simpleName}") }
      }
    }
    guard = Runnable { if (!over) { DriveService.note("musique : catalogue sans réponse"); finish(false) } }
    val b = try {
      MediaBrowser(app, component, object : MediaBrowser.ConnectionCallback() {
        override fun onConnected() {
          if (over) return
          val current = browser ?: return finish(false)
          try { connected(current) { ok -> finish(ok) } } catch (e: Exception) { DriveService.note("musique : catalogue ($e)"); finish(false) }
        }
        override fun onConnectionFailed() { DriveService.note("musique : catalogue refusé à Estompe"); finish(false) }
        override fun onConnectionSuspended() { finish(false) }
      }, null)
    } catch (e: Exception) {
      DriveService.note("musique : connexion au catalogue impossible ($e)")
      return false
    }
    browser = b
    ui.postDelayed(guard, BROWSE_TIMEOUT_MS)
    try { b.connect() } catch (e: Exception) {
      DriveService.note("musique : connexion au catalogue impossible ($e)")
      finish(false)
    }
    return true
  }

  private fun browse(name: String, pkg: String, done: (Boolean) -> Unit) {
    val to = try { target(pkg) } catch (e: Exception) { "" }
    if (to.isEmpty() || name.isBlank()) { DriveService.note("musique : aucune app pour la playlist"); return done(false) }
    var fell = false
    var settled = false
    val fallback = {
      if (!fell) {
        fell = true
        find("playlist $name", to, Bundle().apply {
          putString(MediaStore.EXTRA_MEDIA_FOCUS, MediaStore.Audio.Playlists.ENTRY_CONTENT_TYPE)
          putString(EXTRA_PLAYLIST, name)
        }, done)
      }
    }
    val wanted = try { fold(name) } catch (e: Exception) { name.lowercase() }
    val started = connect(to, { ok -> settled = true; if (ok) done(true) else fallback() }) { browser, finish ->
      val controller = MediaController(app, browser.sessionToken)
      var subscriptions = 0
      fun play(item: MediaBrowser.MediaItem) {
        if (settled) return
        val id = item.mediaId ?: return finish(false)
        controller.transportControls.playFromMediaId(id, null)
        finish(true)
      }
      fun children(parent: String, got: (List<MediaBrowser.MediaItem>?) -> Unit) {
        if (subscriptions >= MAX_SUBSCRIPTIONS) return got(null)
        subscriptions++
        browser.subscribe(parent, object : MediaBrowser.SubscriptionCallback() {
          var answered = false
          override fun onChildrenLoaded(parentId: String, items: MutableList<MediaBrowser.MediaItem>) {
            if (answered) return
            answered = true
            try { browser.unsubscribe(parentId) } catch (e: Exception) {}
            try { got(items) } catch (e: Exception) { DriveService.note("musique : catalogue ($e)"); finish(false) }
          }
          override fun onError(parentId: String) {
            if (answered) return
            answered = true
            try { got(null) } catch (e: Exception) { DriveService.note("musique : catalogue ($e)"); finish(false) }
          }
        })
      }
      fun title(item: MediaBrowser.MediaItem) = try { fold(item.description.title?.toString().orEmpty()) } catch (e: Exception) { "" }
      fun match(items: List<MediaBrowser.MediaItem>) = items.firstOrNull { title(it) == wanted } ?: items.firstOrNull { title(it).let { t -> t.isNotEmpty() && t.contains(wanted) } }
      fun open(item: MediaBrowser.MediaItem) {
        if (item.isPlayable) return play(item)
        val id = item.mediaId ?: return finish(false)
        children(id) { items -> items?.firstOrNull { it.isPlayable }?.let { play(it) } ?: finish(false) }
      }
      children(browser.root) { top ->
        if (top.isNullOrEmpty()) return@children finish(false)
        match(top)?.let { return@children open(it) }
        val folders = top.filter { it.isBrowsable }.mapNotNull { it.mediaId }.take(MAX_SUBSCRIPTIONS - 2)
        if (folders.isEmpty()) return@children finish(false)
        var waiting = folders.size
        var found: MediaBrowser.MediaItem? = null
        for (f in folders) {
          children(f) { items ->
            if (found == null && items != null) found = match(items)
            waiting--
            if (waiting == 0) found?.let { open(it) } ?: finish(false)
          }
        }
      }
    }
    if (!started) fallback()
  }

  companion object {
    private const val BROWSE_TIMEOUT_MS = 8_000L
    private const val LINGER_MS = 5_000L
    private const val PLAY_CHECK_MS = 3_000L
    private const val PLAY_POLL_MS = 300L
    private const val OPEN_WAIT_MS = 6_000L
    private const val MAX_SUBSCRIPTIONS = 8
    private const val EXTRA_PLAYLIST = "android.intent.extra.playlist"
  }
}
