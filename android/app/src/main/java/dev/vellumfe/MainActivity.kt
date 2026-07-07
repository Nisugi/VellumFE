package dev.vellumfe

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import dev.vellumfe.core.VellumCore
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Fullscreen WebView over the embedded web frontend. The Rust core runs in
 * [CoreService]; this activity is just glass. On create it (re)starts the
 * service, boots the core (idempotent), health-polls the local server, and
 * loads `/play#token=...`.
 */
class MainActivity : Activity() {

    private lateinit var webView: WebView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (Build.VERSION.SDK_INT >= 33) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 0)
        }
        startForegroundService(Intent(this, CoreService::class.java))

        webView = WebView(this).apply {
            setBackgroundColor(Color.parseColor("#111318"))
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(
                    view: WebView,
                    request: WebResourceRequest,
                ): Boolean {
                    // Everything except the local server goes to the system
                    // browser (game LaunchURL links, play.net pages).
                    return if (request.url.host != "127.0.0.1") {
                        startActivity(Intent(Intent.ACTION_VIEW, request.url))
                        true
                    } else {
                        false
                    }
                }
            }
        }
        setContentView(webView)
        bootAndLoad()
    }

    /** Boot the core (idempotent), wait for the server, load the client. */
    private fun bootAndLoad() {
        Thread({
            val info = JSONObject(VellumCore.startCore(filesDir.absolutePath))
            if (info.has("error")) {
                showError("Core failed to start:\n${info.optString("error")}")
                return@Thread
            }
            val port = info.getInt("port")
            val token = info.getString("token")
            if (!waitForServer(port)) {
                showError("The embedded server did not come up on port $port.")
                return@Thread
            }
            runOnUiThread {
                webView.loadUrl("http://127.0.0.1:$port/play#token=$token")
            }
        }, "core-boot").start()
    }

    private fun waitForServer(port: Int): Boolean {
        repeat(40) { // ~10s
            try {
                val conn = URL("http://127.0.0.1:$port/health")
                    .openConnection() as HttpURLConnection
                conn.connectTimeout = 500
                conn.readTimeout = 500
                if (conn.responseCode == 200) return true
            } catch (_: Exception) {
                // not up yet
            }
            Thread.sleep(250)
        }
        return false
    }

    private fun showError(message: String) {
        runOnUiThread {
            val html = """
                <html><body style="background:#111318;color:#d6d6d6;
                font-family:monospace;padding:24px;">
                <h3 style="color:#d9534f;">VellumFE</h3>
                <pre style="white-space:pre-wrap;">$message</pre>
                </body></html>
            """.trimIndent()
            webView.loadDataWithBaseURL(null, html, "text/html", "utf-8", null)
        }
    }

    @Deprecated("Deprecated in API 33; fine with legacy back handling")
    override fun onBackPressed() {
        // Back navigates the WebView; at the root it backgrounds the app —
        // never kills it (the service owns the session either way).
        if (webView.canGoBack()) {
            webView.goBack()
        } else {
            moveTaskToBack(true)
        }
    }
}
