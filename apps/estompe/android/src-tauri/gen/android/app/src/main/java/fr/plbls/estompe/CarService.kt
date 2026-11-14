package fr.plbls.estompe

import android.media.MediaDescription
import android.media.MediaMetadata
import android.media.browse.MediaBrowser
import android.media.session.MediaSession
import android.media.session.PlaybackState
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.service.media.MediaBrowserService

class CarService : MediaBrowserService() {
  private val ui = Handler(Looper.getMainLooper())
  private var session: MediaSession? = null

  override fun onCreate() {
    super.onCreate()
    val s = MediaSession(this, "estompe-assistant")
    s.setCallback(object : MediaSession.Callback() {
      override fun onPlay() { act("ask") }
      override fun onPlayFromMediaId(mediaId: String?, extras: Bundle?) { act(mediaId ?: "ask") }
      override fun onPlayFromSearch(query: String?, extras: Bundle?) { act(if (query.isNullOrBlank()) "ask" else "say:$query") }
      override fun onPause() { act("stop") }
      override fun onStop() { act("stop") }
    })
    session = s
    sessionToken = s.sessionToken
    instance = this
    render("", "", false)
  }

  override fun onDestroy() {
    if (instance === this) instance = null
    try { session?.release() } catch (e: Exception) { DriveService.note("écran voiture : ${e.javaClass.simpleName}") }
    session = null
    super.onDestroy()
  }

  override fun onGetRoot(clientPackageName: String, clientUid: Int, rootHints: Bundle?): BrowserRoot? =
    if (clientPackageName in CLIENTS || clientPackageName == packageName) BrowserRoot(ROOT, null) else null

  override fun onLoadChildren(parentId: String, result: Result<MutableList<MediaBrowser.MediaItem>>) {
    val items = ArrayList<MediaBrowser.MediaItem>()
    if (parentId == ROOT) for ((id, title, subtitle) in ACTIONS) {
      items.add(MediaBrowser.MediaItem(MediaDescription.Builder().setMediaId(id).setTitle(title).setSubtitle(subtitle).build(), MediaBrowser.MediaItem.FLAG_PLAYABLE))
    }
    result.sendResult(items)
  }

  private fun act(action: String) {
    if (action == "stop") { AssistantService.remote("stop"); return }
    if (AssistantService.busyNow) return
    if (!AssistantService.remote(action)) render("Assistant arrêté", "Ouvre Estompe sur le téléphone pour relancer l'écoute.", false, true)
  }

  private fun render(title: String, text: String, active: Boolean, error: Boolean = false) {
    val s = session ?: return
    try {
      s.setMetadata(MediaMetadata.Builder().putString(MediaMetadata.METADATA_KEY_TITLE, title.ifEmpty { "Assistant vocal" })
        .putString(MediaMetadata.METADATA_KEY_ARTIST, text.ifEmpty { "Touche lecture ou dis « Estompe »." }).build())
      val state = PlaybackState.Builder().setActions(PlaybackState.ACTION_PLAY or PlaybackState.ACTION_PLAY_FROM_MEDIA_ID or PlaybackState.ACTION_PLAY_FROM_SEARCH or
        PlaybackState.ACTION_PAUSE or PlaybackState.ACTION_STOP)
      if (error) state.setState(PlaybackState.STATE_ERROR, 0, 0f).setErrorMessage(text)
      else state.setState(if (active) PlaybackState.STATE_PLAYING else PlaybackState.STATE_PAUSED, 0, if (active) 1f else 0f)
      s.setPlaybackState(state.build())
      s.isActive = active
    } catch (e: Exception) {
      DriveService.note("écran voiture : ${e.javaClass.simpleName}")
    }
  }

  companion object {
    private const val ROOT = "estompe"
    private val CLIENTS = setOf("com.google.android.projection.gearhead", "com.google.android.carassistant", "com.google.android.googlequicksearchbox")
    private val ACTIONS = listOf(
      Triple("ask", "Donner un ordre", "Estompe répond « Oui ? » puis écoute"),
      Triple("unread", "Lire mes messages", "Les nouveaux messages, cinq au plus"),
      Triple("last", "Lire le dernier message", "Le dernier message annoncé ou reçu"),
    )
    @Volatile private var instance: CarService? = null

    fun show(title: String, text: String, active: Boolean) {
      val s = instance ?: return
      s.ui.post { s.render(title, text, active) }
    }
  }
}
