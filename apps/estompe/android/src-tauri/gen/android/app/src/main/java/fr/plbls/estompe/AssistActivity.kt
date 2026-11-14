package fr.plbls.estompe

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper

class AssistActivity : Activity() {
  private val ui = Handler(Looper.getMainLooper())
  private var asked = false

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (Build.VERSION.SDK_INT >= 27) setShowWhenLocked(true)
  }

  override fun onNewIntent(intent: Intent?) {
    super.onNewIntent(intent)
    asked = false
  }

  override fun onResume() {
    super.onResume()
    if (asked) return
    asked = true
    ui.removeCallbacksAndMessages(null)
    if (AssistantService.ask(this)) {
      ui.postDelayed({ finish() }, CLOSE_MS)
      return
    }
    try { startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } catch (e: Exception) {
      DriveService.note("assistant sur demande : ouverture d'Estompe ${e.javaClass.simpleName}")
    }
    finish()
  }

  override fun onDestroy() {
    ui.removeCallbacksAndMessages(null)
    super.onDestroy()
  }

  companion object {
    private const val CLOSE_MS = 2_000L
  }
}
