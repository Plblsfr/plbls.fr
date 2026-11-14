package fr.plbls.estompe

import android.animation.ObjectAnimator
import android.animation.PropertyValuesHolder
import android.animation.ValueAnimator
import android.content.Context
import android.content.res.Configuration
import android.graphics.PixelFormat
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView

enum class OverlayState(val caption: String) { LISTEN("À l'écoute"), HEARD("Entendu"), SPEAK("Estompe") }

class AssistantOverlay(context: Context) {
  private val app = context.applicationContext
  private val ui = Handler(Looper.getMainLooper())
  private val windows = app.getSystemService(WindowManager::class.java)
  private var root: FrameLayout? = null
  private var caption: TextView? = null
  private var body: TextView? = null
  private var badge: View? = null
  private var pulse: ObjectAnimator? = null
  private val later = Runnable { leave() }

  fun show(text: String, state: OverlayState = OverlayState.SPEAK) {
    ui.removeCallbacks(later)
    if (Build.VERSION.SDK_INT < 26 || windows == null || !allowed(app)) return remove()
    try {
      if (root == null) enter()
      root?.animate()?.cancel()
      root?.alpha = 1f
      caption?.text = state.caption
      body?.text = text
      beat(state == OverlayState.LISTEN)
    } catch (e: Exception) {
      DriveService.note("assistant : affichage impossible (${e.javaClass.simpleName})")
      remove()
    }
  }

  fun hide(afterMs: Long = 0) {
    ui.removeCallbacks(later)
    if (afterMs <= 0) remove() else ui.postDelayed(later, afterMs)
  }

  private fun leave() {
    val r = root ?: return
    beat(false)
    r.animate().alpha(0f).setDuration(160).withEndAction { remove() }.start()
  }

  private fun remove() {
    beat(false)
    val r = root ?: return
    root = null
    caption = null
    body = null
    badge = null
    try { r.animate().cancel(); windows?.removeView(r) } catch (e: Exception) { DriveService.note("assistant : retrait de l'affichage (${e.javaClass.simpleName})") }
  }

  private fun beat(on: Boolean) {
    pulse?.cancel()
    pulse = null
    val b = badge ?: return
    b.scaleX = 1f
    b.scaleY = 1f
    if (!on) return
    pulse = ObjectAnimator.ofPropertyValuesHolder(b, PropertyValuesHolder.ofFloat(View.SCALE_X, 1f, 1.14f), PropertyValuesHolder.ofFloat(View.SCALE_Y, 1f, 1.14f)).apply {
      duration = 620
      repeatMode = ValueAnimator.REVERSE
      repeatCount = ValueAnimator.INFINITE
      start()
    }
  }

  private fun dp(n: Int) = (n * app.resources.displayMetrics.density).toInt()

  private fun enter() {
    val night = app.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK == Configuration.UI_MODE_NIGHT_YES
    val surface = if (night) 0xFF1E1B19.toInt() else 0xFFFFFDFB.toInt()
    val ink = if (night) 0xFFF1ECE6.toInt() else 0xFF16130F.toInt()
    val grey = if (night) 0xFFA9A097.toInt() else 0xFF6A635C.toInt()
    val line = if (night) 0xFF433D38.toInt() else 0xFFE2DAD2.toInt()
    val voice = if (night) 0xFFE9849F.toInt() else 0xFFC2185B.toInt()
    val soft = if (night) 0xFF3A2029.toInt() else 0xFFFBE3EB.toInt()
    val icon = ImageView(app).apply {
      setImageResource(R.drawable.ic_notification)
      setColorFilter(voice)
      background = GradientDrawable().apply { setColor(soft); cornerRadius = dp(14).toFloat() }
      setPadding(dp(10), dp(10), dp(10), dp(10))
    }
    val small = TextView(app).apply {
      setTextColor(grey)
      setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
      typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
      letterSpacing = 0.06f
      isAllCaps = true
      maxLines = 1
    }
    val main = TextView(app).apply {
      setTextColor(ink)
      setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
      setLineSpacing(dp(2).toFloat(), 1f)
      maxLines = 3
      ellipsize = TextUtils.TruncateAt.END
    }
    val texts = LinearLayout(app).apply {
      orientation = LinearLayout.VERTICAL
      addView(small)
      addView(main, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT).apply { topMargin = dp(2) })
    }
    val card = LinearLayout(app).apply {
      orientation = LinearLayout.HORIZONTAL
      gravity = Gravity.CENTER_VERTICAL
      setPadding(dp(14), dp(12), dp(18), dp(14))
      background = GradientDrawable().apply { setColor(surface); cornerRadius = dp(20).toFloat(); setStroke(dp(1), line) }
      elevation = dp(10).toFloat()
      addView(icon, LinearLayout.LayoutParams(dp(44), dp(44)))
      addView(texts, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f).apply { marginStart = dp(14) })
      setOnClickListener { hide() }
    }
    val width = minOf(app.resources.displayMetrics.widthPixels, dp(460))
    val frame = FrameLayout(app).apply {
      setPadding(dp(12), dp(8), dp(12), dp(20))
      clipToPadding = false
      addView(card, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.WRAP_CONTENT))
      alpha = 0f
      translationY = -dp(14).toFloat()
    }
    val params = WindowManager.LayoutParams(width, WindowManager.LayoutParams.WRAP_CONTENT,
      if (Build.VERSION.SDK_INT >= 26) WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY else 0,
      WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL, PixelFormat.TRANSLUCENT).apply {
      gravity = Gravity.TOP or Gravity.CENTER_HORIZONTAL
      y = dp(40)
    }
    windows?.addView(frame, params)
    root = frame
    caption = small
    body = main
    badge = icon
    frame.animate().alpha(1f).translationY(0f).setDuration(200).start()
  }

  companion object {
    fun allowed(c: Context): Boolean = try { Settings.canDrawOverlays(c) } catch (e: Exception) { false }
  }
}
