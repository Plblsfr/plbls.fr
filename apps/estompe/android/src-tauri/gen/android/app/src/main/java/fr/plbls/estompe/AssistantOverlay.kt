package fr.plbls.estompe

import android.content.Context
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.WindowManager
import android.widget.TextView

class AssistantOverlay(context: Context) {
  private val app = context.applicationContext
  private val ui = Handler(Looper.getMainLooper())
  private val windows = app.getSystemService(WindowManager::class.java)
  private var view: TextView? = null
  private val later = Runnable { remove() }

  fun show(text: String) {
    ui.removeCallbacks(later)
    if (Build.VERSION.SDK_INT < 26 || windows == null || !allowed(app)) return remove()
    try {
      val v = view ?: create().also { windows.addView(it, params()); view = it }
      v.text = text
    } catch (e: Exception) {
      DriveService.note("assistant : affichage impossible (${e.javaClass.simpleName})")
      remove()
    }
  }

  fun hide(afterMs: Long = 0) {
    ui.removeCallbacks(later)
    if (afterMs <= 0) remove() else ui.postDelayed(later, afterMs)
  }

  private fun remove() {
    val v = view ?: return
    view = null
    try { windows?.removeView(v) } catch (e: Exception) { DriveService.note("assistant : retrait de l'affichage (${e.javaClass.simpleName})") }
  }

  private fun dp(n: Int) = (n * app.resources.displayMetrics.density).toInt()

  private fun create(): TextView = TextView(app).apply {
    setTextColor(0xFFFFFFFF.toInt())
    setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
    setPadding(dp(18), dp(12), dp(18), dp(12))
    maxLines = 4
    ellipsize = TextUtils.TruncateAt.END
    maxWidth = app.resources.displayMetrics.widthPixels - dp(32)
    background = GradientDrawable().apply { setColor(0xF01B1B1F.toInt()); cornerRadius = dp(22).toFloat(); setStroke(dp(1), 0x55FFFFFF) }
    setOnClickListener { hide() }
  }

  private fun params() = WindowManager.LayoutParams(WindowManager.LayoutParams.WRAP_CONTENT, WindowManager.LayoutParams.WRAP_CONTENT,
    if (Build.VERSION.SDK_INT >= 26) WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY else 0,
    WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL, PixelFormat.TRANSLUCENT).apply {
    gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
    y = dp(56)
  }

  companion object {
    fun allowed(c: Context): Boolean = try { Settings.canDrawOverlays(c) } catch (e: Exception) { false }
  }
}
