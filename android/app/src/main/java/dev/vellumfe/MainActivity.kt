package dev.vellumfe

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.Settings
import android.util.Log
import android.webkit.ConsoleMessage
import android.webkit.WebChromeClient
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

    /** Set once the server is up; lets a deep link rebuild the boot URL. */
    private var bootPort = -1
    private var bootToken: String? = null

    /** Fragment tail from a vellum:// deep link; rides the boot URL so the
     * web client prefills the Lich login tab. */
    private var lichFragment: String? = null

    /** Prefill tail from a vellum://remote deep link (rhost/rport/rkey);
     * rides the boot URL so the web client opens the Remote tab. Never
     * auto-connects — the user presses Connect on the login page. */
    private var remoteFragment: String? = null

    /** The remembered remote server (Keystore-sealed). Its address (never
     * the token) rides the boot URL so the login page can offer one-tap
     * connect; Connect/Forget come back as vellum://remote/… actions. */
    private var savedRemote: RemoteStore.Target? = null

    /** Remote host the WebView may browse in-app (Remote mode); null while
     * on the embedded core. Everything else non-loopback goes external. */
    private var allowedRemoteHost: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        lichFragment = lichFragmentFrom(intent)
        remoteFragment = remoteFragmentFrom(intent)

        if (Build.VERSION.SDK_INT >= 33) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 0)
        }
        requestBatteryExemptionOnce()
        startForegroundService(Intent(this, CoreService::class.java))

        Log.i(TAG, "WebView engine: ${WebView.getCurrentWebViewPackage()?.let { "${it.packageName} ${it.versionName}" } ?: "unknown"}")

        webView = WebView(this).apply {
            setBackgroundColor(Color.parseColor("#111318"))
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            // Surface page JS errors in logcat: an engine too old for the
            // client's JavaScript otherwise fails as a silent static page.
            webChromeClient = object : WebChromeClient() {
                override fun onConsoleMessage(message: ConsoleMessage): Boolean {
                    Log.i(
                        TAG,
                        "js[${message.messageLevel()}] ${message.sourceId()}:${message.lineNumber()} ${message.message()}",
                    )
                    return true
                }
            }
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(
                    view: WebView,
                    request: WebResourceRequest,
                ): Boolean {
                    val url = request.url
                    // vellum:// navigations are shell actions from the page
                    // (Remote tab: pair/connect/forget/back-to-local).
                    if (url.scheme == "vellum") {
                        handleShellUrl(url)
                        return true
                    }
                    // The local server and the paired remote host browse
                    // in-app; everything else goes to the system browser
                    // (game LaunchURL links, play.net pages).
                    val host = url.host?.lowercase()
                    return if (host == "127.0.0.1" || (allowedRemoteHost != null && host == allowedRemoteHost)) {
                        false
                    } else {
                        startActivity(Intent(Intent.ACTION_VIEW, url))
                        true
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
            CryptoKeys.installPasswordKey(this)
            savedRemote = RemoteStore.load(this)
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
                bootPort = port
                bootToken = token
                webView.loadUrl(bootUrl(port, token))
            }
        }, "core-boot").start()
    }

    private fun bootUrl(port: Int, token: String): String {
        // app=1 marks the shell for the web client: it reveals the Remote
        // login tab (whose actions only a shell can catch).
        var url = "http://127.0.0.1:$port/play#token=$token&app=1"
        savedRemote?.let { url += "&remote=" + Uri.encode("${it.host}:${it.port}") }
        remoteFragment?.let { url += "&$it" }
        lichFragment?.let { url += "&$it" }
        return url
    }

    /** Reload the local boot URL (embedded login page); no-op while boot
     * is still in flight — it picks the fragments up. */
    private fun showLocal() {
        allowedRemoteHost = null
        val port = bootPort
        val token = bootToken
        if (port > 0 && token != null) {
            runOnUiThread { webView.loadUrl(bootUrl(port, token)) }
        }
    }

    /** Point the WebView at a desktop VellumFE's dashboard. The embedded
     * core keeps running but sits idle — there is no in-app game socket in
     * this mode; the web client's own reconnect handles resume. */
    private fun showRemote(target: RemoteStore.Target) {
        allowedRemoteHost = target.host.lowercase()
        // Bracket bare IPv6 literals so the URL parses.
        val host = if (target.host.contains(":") && !target.host.startsWith("[")) {
            "[${target.host}]"
        } else {
            target.host
        }
        val fragment = if (target.token.isEmpty()) "app=1" else "token=${target.token}&app=1"
        runOnUiThread { webView.loadUrl("http://$host:${target.port}/#$fragment") }
    }

    /** vellum:// navigations from the page itself (Remote tab actions). */
    private fun handleShellUrl(uri: Uri) {
        when (uri.host) {
            "local" -> showLocal()
            "remote" -> when (uri.path.orEmpty()) {
                "", "/" -> {
                    // Pair: vellum://remote?host&port[&token][&save=0]
                    val target = remoteTargetFrom(uri) ?: return
                    if (uri.getQueryParameter("save") != "0") {
                        RemoteStore.save(this, target)
                        savedRemote = target
                    }
                    showRemote(target)
                }
                "/connect" -> savedRemote?.let { showRemote(it) }
                "/forget" -> {
                    RemoteStore.forget(this)
                    savedRemote = null
                    showLocal()
                }
            }
        }
    }

    private fun remoteTargetFrom(uri: Uri): RemoteStore.Target? {
        val host = uri.getQueryParameter("host")?.trim().orEmpty()
        val port = uri.getQueryParameter("port")?.trim()?.toIntOrNull()
        if (host.isEmpty() || port == null || port !in 1..65535) return null
        return RemoteStore.Target(
            host = host,
            port = port,
            token = uri.getQueryParameter("token")?.trim().orEmpty(),
        )
    }

    /** vellum://lich?host=…&port=…[&name=…] → the #lich= fragment the web
     * client prefills its Lich tab from; null for anything else. */
    private fun lichFragmentFrom(intent: Intent?): String? {
        val uri = intent?.data ?: return null
        if (uri.scheme != "vellum" || uri.host != "lich") return null
        val host = uri.getQueryParameter("host")?.trim().orEmpty()
        val port = uri.getQueryParameter("port")?.trim()?.toIntOrNull()
        if (host.isEmpty() || port == null || port !in 1..65535) return null
        var fragment = "lich=" + Uri.encode("$host:$port")
        uri.getQueryParameter("name")?.trim()?.takeIf { it.isNotEmpty() }?.let {
            fragment += "&name=" + Uri.encode(it)
        }
        return fragment
    }

    /** vellum://remote?host=…&port=…[&token=…] → the #rhost=…&rport=…
     * [&rkey=…] prefill tail for the web client's Remote tab. ("rkey": the
     * client's token regex is unanchored, so any *token= param in the
     * local fragment would be eaten by it.) Prefill only — a malicious QR
     * can't point the app at an attacker's server unseen. */
    private fun remoteFragmentFrom(intent: Intent?): String? {
        val uri = intent?.data ?: return null
        if (uri.scheme != "vellum" || uri.host != "remote") return null
        if (uri.path.orEmpty() !in listOf("", "/")) return null
        val target = remoteTargetFrom(uri) ?: return null
        var fragment = "rhost=" + Uri.encode(target.host) + "&rport=${target.port}"
        if (target.token.isNotEmpty()) {
            fragment += "&rkey=" + Uri.encode(target.token)
        }
        return fragment
    }

    /** singleTask: a deep link while running lands here instead of a fresh
     * activity. Reload with the new fragment; the client's resume flow
     * restores scrollback if a session is live. */
    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        val lich = lichFragmentFrom(intent)?.also { lichFragment = it }
        val remote = remoteFragmentFrom(intent)?.also { remoteFragment = it }
        if (lich == null && remote == null) return
        // Back to the embedded login page with the target prefilled (even
        // if the WebView was on a remote server).
        showLocal()
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

    /**
     * Ask once for a battery-optimization exemption: Doze can throttle the
     * network mid-session even with the wakelock held. Only prompts when
     * not already exempt, and never re-prompts a user who said no (the
     * dialog is available any time under system battery settings).
     */
    private fun requestBatteryExemptionOnce() {
        val prefs = getSharedPreferences("vellum", MODE_PRIVATE)
        val pm = getSystemService(POWER_SERVICE) as PowerManager
        if (pm.isIgnoringBatteryOptimizations(packageName)) return
        if (prefs.getBoolean("battery_prompted", false)) return
        prefs.edit().putBoolean("battery_prompted", true).apply()
        try {
            startActivity(
                Intent(
                    Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                    Uri.parse("package:$packageName"),
                ),
            )
        } catch (e: Exception) {
            Log.w(TAG, "battery exemption dialog unavailable: $e")
        }
    }

    companion object {
        private const val TAG = "VellumShell"
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
