package fr.plbls.estompe

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
import android.provider.MediaStore
import android.service.media.MediaBrowserService
import android.view.KeyEvent
import java.text.Collator
import java.util.Locale

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

  fun transport(action: Transport, app: String): Boolean = try {
    when (action) {
      Transport.VOLUME_UP -> volume(AudioManager.ADJUST_RAISE)
      Transport.VOLUME_DOWN -> volume(AudioManager.ADJUST_LOWER)
      Transport.PLAY -> controller(app)?.let { it.transportControls.play(); true } ?: playKey(app)
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

  private fun once(done: (Boolean) -> Unit): (Boolean) -> Unit {
    var called = false
    return { ok ->
      val run = { if (!called) { called = true; try { done(ok) } catch (e: Exception) { DriveService.note("musique : rappel ${e.javaClass.simpleName}") } } }
      if (Looper.myLooper() == Looper.getMainLooper()) run() else ui.post { run() }
    }
  }

  private fun sessions(): List<MediaController> = try {
    app.getSystemService(MediaSessionManager::class.java)?.getActiveSessions(AssistantListener.component(app)).orEmpty()
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
    if (pkg.isNotEmpty()) {
      val receiver = try {
        @Suppress("DEPRECATION")
        app.packageManager.queryBroadcastReceivers(Intent(Intent.ACTION_MEDIA_BUTTON).setPackage(pkg), 0).firstOrNull()?.activityInfo
      } catch (e: Exception) { null }
      if (receiver != null) {
        val to = ComponentName(receiver.packageName, receiver.name)
        app.sendBroadcast(Intent(Intent.ACTION_MEDIA_BUTTON).setComponent(to).putExtra(Intent.EXTRA_KEY_EVENT, down))
        app.sendBroadcast(Intent(Intent.ACTION_MEDIA_BUTTON).setComponent(to).putExtra(Intent.EXTRA_KEY_EVENT, up))
        return true
      }
    }
    val audio = app.getSystemService(AudioManager::class.java) ?: return false
    audio.dispatchMediaKeyEvent(down)
    audio.dispatchMediaKeyEvent(up)
    return true
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
      if (startSearch(query, to, extras)) return done(true)
      connect(to, { done(false) }) { browser, finish ->
        try {
          MediaController(app, browser.sessionToken).transportControls.playFromSearch(query, extras)
          finish(true)
        } catch (e: Exception) {
          DriveService.note("musique : recherche refusée par le catalogue ($e)")
          finish(false)
        }
      }.let { if (!it) done(false) }
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

  private fun connect(pkg: String, failed: () -> Unit, connected: (MediaBrowser, (Boolean) -> Unit) -> Unit): Boolean {
    val component = browserComponent(pkg) ?: return false
    var browser: MediaBrowser? = null
    var over = false
    lateinit var guard: Runnable
    fun close() {
      ui.removeCallbacks(guard)
      try { browser?.disconnect() } catch (e: Exception) { DriveService.note("musique : déconnexion du catalogue ($e)") }
      browser = null
    }
    val finish: (Boolean) -> Unit = { ok -> if (!over) { over = true; close(); if (!ok) failed() } }
    guard = Runnable { if (!over) { DriveService.note("musique : catalogue sans réponse"); finish(false) } }
    return try {
      val b = MediaBrowser(app, component, object : MediaBrowser.ConnectionCallback() {
        override fun onConnected() {
          if (over) return
          val current = browser ?: return
          try { connected(current) { ok -> finish(ok) } } catch (e: Exception) { DriveService.note("musique : catalogue ($e)"); finish(false) }
        }
        override fun onConnectionFailed() { DriveService.note("musique : catalogue refusé à Estompe"); finish(false) }
        override fun onConnectionSuspended() { finish(false) }
      }, null)
      browser = b
      ui.postDelayed(guard, BROWSE_TIMEOUT_MS)
      b.connect()
      true
    } catch (e: Exception) {
      DriveService.note("musique : connexion au catalogue impossible ($e)")
      over = true
      close()
      false
    }
  }

  private fun browse(name: String, pkg: String, done: (Boolean) -> Unit) {
    val to = try { target(pkg) } catch (e: Exception) { "" }
    if (to.isEmpty() || name.isBlank()) { DriveService.note("musique : aucune app pour la playlist"); return done(false) }
    var played = false
    val fallback = {
      if (!played) {
        played = true
        find("playlist $name", to, Bundle().apply {
          putString(MediaStore.EXTRA_MEDIA_FOCUS, MediaStore.Audio.Playlists.ENTRY_CONTENT_TYPE)
          putString(EXTRA_PLAYLIST, name)
        }, done)
      }
    }
    val wanted = try { fold(name) } catch (e: Exception) { name.lowercase() }
    val started = connect(to, { fallback() }) { browser, finish ->
      val controller = MediaController(app, browser.sessionToken)
      var subscriptions = 0
      fun play(item: MediaBrowser.MediaItem) {
        if (played) return
        val id = item.mediaId ?: return finish(false)
        controller.transportControls.playFromMediaId(id, null)
        played = true
        finish(true)
        done(true)
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
            got(items)
          }
          override fun onError(parentId: String) {
            if (answered) return
            answered = true
            got(null)
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
    private const val MAX_SUBSCRIPTIONS = 8
    private const val EXTRA_PLAYLIST = "android.intent.extra.playlist"
  }
}
