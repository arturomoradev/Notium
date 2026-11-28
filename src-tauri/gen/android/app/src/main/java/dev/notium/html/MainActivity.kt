package dev.notium.html

import android.content.pm.ApplicationInfo
import android.graphics.Color
import android.graphics.drawable.ColorDrawable
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.graphics.ColorUtils
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import org.json.JSONObject

class MainActivity : TauriActivity() {
  private val tag = "Notium"
  private var targetRefreshRate = 120f
  private var activeWebView: WebView? = null
  private var pendingExportBytes: ByteArray? = null

  // Wry's default back handling only walks WebView history, which this
  // single-page app does not use; back is routed to the web layer instead.
  override val handleBackNavigation: Boolean = false

  private val createDocumentLauncher = registerForActivityResult(
    ActivityResultContracts.CreateDocument("application/json")
  ) { uri ->
    val bytes = pendingExportBytes
    pendingExportBytes = null
    if (uri != null && bytes != null) {
      try {
        contentResolver.openOutputStream(uri)?.use { out ->
          out.write(bytes)
          out.flush()
        }
        notifyExportResult(true, uri.lastPathSegment ?: "Archivo guardado")
      } catch (e: Exception) {
        notifyExportResult(false, e.message ?: "Error al escribir archivo")
      }
    } else {
      notifyExportResult(false, "Cancelado por el usuario")
    }
  }

  private fun notifyExportResult(ok: Boolean, message: String) {
    val payload = JSONObject().apply {
      put("ok", ok)
      put("message", message)
    }.toString()
    activeWebView?.post {
      activeWebView?.evaluateJavascript(
        "window.dispatchEvent(new CustomEvent('notium:android-export-result', { detail: $payload }));",
        null
      )
    }
  }

  inner class AndroidFileBridge {
    @JavascriptInterface
    fun saveJsonDocument(filename: String, content: String) {
      runOnUiThread {
        pendingExportBytes = content.toByteArray(Charsets.UTF_8)
        createDocumentLauncher.launch(filename)
      }
    }

    /**
     * Paints the window background shown behind the transparent system bars and
     * picks dark or light bar icons to match. Accepts only `#rrggbb`.
     */
    @JavascriptInterface
    fun setSystemBarColor(hex: String) {
      if (!Regex("^#[0-9a-fA-F]{6}$").matches(hex)) return
      val color = Color.parseColor(hex)
      val isLight = ColorUtils.calculateLuminance(color) > 0.5
      runOnUiThread {
        window.setBackgroundDrawable(ColorDrawable(color))
        val controller = WindowCompat.getInsetsController(window, window.decorView)
        controller.isAppearanceLightStatusBars = isLight
        controller.isAppearanceLightNavigationBars = isLight
      }
    }
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    Log.i(tag, "ANDROID_LIFECYCLE onCreate ts=${System.currentTimeMillis()}")
    // Only enable WebView remote debugging in debuggable builds.
    val isDebuggable = (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) != 0
    WebView.setWebContentsDebuggingEnabled(isDebuggable)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    requestHighestRefreshRate()
    applySystemBarInsets()
    registerBackNavigation()
  }

  /**
   * Back button / back gesture: ask the frontend to close its top-most layer
   * (menu, modal, settings page, drawer, open note). Only when nothing is left
   * to close does the default behaviour (leaving the app) run.
   */
  private fun registerBackNavigation() {
    val callback = object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        val webView = activeWebView
        if (webView == null) {
          leaveApp()
          return
        }
        webView.evaluateJavascript(
          "(function(){try{return !!(window.__notiumHandleBack&&window.__notiumHandleBack());}catch(e){return false;}})()"
        ) { handled -> if (handled != "true") leaveApp() }
      }

      private fun leaveApp() {
        isEnabled = false
        onBackPressedDispatcher.onBackPressed()
        isEnabled = true
      }
    }
    onBackPressedDispatcher.addCallback(this, callback)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    activeWebView = webView
    webView.addJavascriptInterface(AndroidFileBridge(), "NotiumAndroid")
    // Android 15+ (API 35) frame-rate hint: request the highest refresh rate
    // for the view that actually renders the editor surface.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.VANILLA_ICE_CREAM) {
      webView.setRequestedFrameRate(targetRefreshRate)
      Log.i(
        tag,
        "ANDROID_DISPLAY webView.requestedFrameRate=${webView.requestedFrameRate} target=$targetRefreshRate",
      )
    }
  }

  override fun onResume() {
    super.onResume()
    Log.i(tag, "ANDROID_LIFECYCLE onResume ts=${System.currentTimeMillis()}")
    // OEMs can reset the preferred mode across resume; re-assert it cheaply.
    requestHighestRefreshRate()
  }

  override fun onPause() {
    Log.i(tag, "ANDROID_LIFECYCLE onPause ts=${System.currentTimeMillis()}")
    super.onPause()
  }

  @Suppress("DEPRECATION")
  private fun requestHighestRefreshRate() {
    val display = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      display
    } else {
      windowManager.defaultDisplay
    }
    val modes = display?.supportedModes
    if (modes == null || modes.isEmpty()) {
      Log.w(tag, "ANDROID_DISPLAY no supported display modes reported")
      return
    }

    val current = display.mode
    val highRefresh = modes.filter { it.refreshRate >= 119f }
    val sameResolution = highRefresh.filter {
      it.physicalWidth == current.physicalWidth && it.physicalHeight == current.physicalHeight
    }
    val best = (sameResolution.ifEmpty { highRefresh }.ifEmpty { modes.toList() })
      .maxByOrNull { it.refreshRate } ?: return

    targetRefreshRate = best.refreshRate
    val attrs = window.attributes
    if (attrs.preferredDisplayModeId != best.modeId ||
      attrs.preferredRefreshRate != best.refreshRate
    ) {
      attrs.preferredDisplayModeId = best.modeId
      attrs.preferredRefreshRate = best.refreshRate
      window.attributes = attrs
      Log.i(
        tag,
        "ANDROID_DISPLAY preferredDisplayModeId=${best.modeId} fps=${best.refreshRate} " +
          "resolution=${best.physicalWidth}x${best.physicalHeight}",
      )
    }
  }

  /**
   * Apply system-bar + IME insets to the content container so the WebView
   * layout viewport (and therefore `window.innerHeight` / `100vh` in the
   * frontend) stays clear of the status bar, camera cutout, navigation bar and
   * the on-screen keyboard:
   *   - top       = status bar top inset (Android 15/16 edge-to-edge)
   *   - bottom    = max(navigation bar, IME) bottom inset (keyboard over nav bar)
   *   - left/right = system bar side insets (landscape cutouts)
   * On Android 15+ `adjustResize` is a no-op so the IME inset is required; on
   * older releases `adjustResize` already shrinks the window, in which case the
   * IME inset reports 0 and we simply fall back to the navigation bar inset.
   */
  private fun applySystemBarInsets() {
    val content = findViewById<ViewGroup>(android.R.id.content) ?: return
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      val top = bars.top
      val bottom = maxOf(bars.bottom, ime.bottom)
      val left = bars.left
      val right = bars.right
      if (view.paddingTop != top || view.paddingBottom != bottom ||
        view.paddingLeft != left || view.paddingRight != right
      ) {
        view.setPadding(left, top, right, bottom)
        Log.i(
          tag,
          "ANDROID_INSETS top=$top bottom=$bottom left=$left right=$right " +
            "imeVisible=${insets.isVisible(WindowInsetsCompat.Type.ime())}",
        )
      }
      insets
    }
    ViewCompat.requestApplyInsets(content)
  }
}
