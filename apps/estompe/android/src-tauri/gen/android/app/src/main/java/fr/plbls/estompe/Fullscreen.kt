package fr.plbls.estompe

import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.view.View
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.GeolocationPermissions
import android.webkit.JsPromptResult
import android.webkit.JsResult
import android.webkit.PermissionRequest
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat

class FullscreenChrome(private val activity: ComponentActivity, private val base: WebChromeClient) : WebChromeClient() {
  private var shown: View? = null
  private var done: CustomViewCallback? = null
  private val back = object : OnBackPressedCallback(false) {
    override fun handleOnBackPressed() {
      val callback = done
      hide()
      callback?.onCustomViewHidden()
    }
  }

  init {
    activity.onBackPressedDispatcher.addCallback(activity, back)
  }

  private fun bars() = WindowCompat.getInsetsController(activity.window, activity.window.decorView)

  override fun onShowCustomView(view: View, callback: CustomViewCallback) {
    if (shown != null) {
      callback.onCustomViewHidden()
      return
    }
    val full = ViewGroup.LayoutParams.MATCH_PARENT
    val box = FrameLayout(activity)
    box.setBackgroundColor(Color.BLACK)
    box.addView(view, FrameLayout.LayoutParams(full, full))
    ViewCompat.setOnApplyWindowInsetsListener(box) { v, insets ->
      val i = insets.getInsets(WindowInsetsCompat.Type.displayCutout())
      v.setPadding(i.left, i.top, i.right, i.bottom)
      WindowInsetsCompat.CONSUMED
    }
    (activity.window.decorView as ViewGroup).addView(box, ViewGroup.LayoutParams(full, full))
    shown = box
    done = callback
    back.isEnabled = true
    bars().systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
    bars().hide(WindowInsetsCompat.Type.systemBars())
  }

  override fun onHideCustomView() = hide()

  private fun hide() {
    val box = shown ?: return
    shown = null
    done = null
    back.isEnabled = false
    (activity.window.decorView as ViewGroup).removeView(box)
    bars().show(WindowInsetsCompat.Type.systemBars())
  }

  override fun onPermissionRequest(request: PermissionRequest) = base.onPermissionRequest(request)
  override fun getDefaultVideoPoster(): Bitmap? = base.defaultVideoPoster
  override fun onJsAlert(view: WebView, url: String, message: String, result: JsResult): Boolean = base.onJsAlert(view, url, message, result)
  override fun onJsConfirm(view: WebView, url: String, message: String, result: JsResult): Boolean = base.onJsConfirm(view, url, message, result)
  override fun onJsPrompt(view: WebView, url: String, message: String, defaultValue: String, result: JsPromptResult): Boolean = base.onJsPrompt(view, url, message, defaultValue, result)
  override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) = base.onGeolocationPermissionsShowPrompt(origin, callback)
  override fun onShowFileChooser(webView: WebView, filePathCallback: ValueCallback<Array<Uri?>?>, fileChooserParams: FileChooserParams): Boolean = base.onShowFileChooser(webView, filePathCallback, fileChooserParams)
  override fun onConsoleMessage(consoleMessage: ConsoleMessage): Boolean = base.onConsoleMessage(consoleMessage)
  override fun onReceivedTitle(view: WebView, title: String) = base.onReceivedTitle(view, title)
}
