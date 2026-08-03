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

    /** Fragment tail from a vellum://lich deep link; rides the boot URL so
     * the web client prefills the Lich login tab. (Remote deep links no
     * longer prefill the page — the native picker owns remote servers.) */
    private var lichFragment: String? = null

    /** Remote host the WebView may browse in-app (Remote mode); null while
     * on the embedded core. Everything else non-loopback goes external. */
    private var allowedRemoteHost: String? = null

    /** True once the embedded core is up (local play started). Lets the
     * picker offer "play on this phone" without restarting the core. */
    private var coreStarted = false

    /** The native character picker, shown at launch when servers are saved. */
    private var picker: RemotePickerView? = null

    /** True while the picker (not the WebView) is the current content view. */
    private var showingPicker = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        lichFragment = lichFragmentFrom(intent)
        // A vellum://remote deep link arriving at launch (system-camera scan)
        // is handled directly (add + connect) rather than parked on the picker.
        val remoteDeepLink = remoteTargetFromIntent(intent)

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
            // Sound alerts and the login music fire from JS without a user
            // gesture — mirrors mediaTypesRequiringUserActionForPlayback = []
            // in the iOS shell's WebViewContainer.
            settings.mediaPlaybackRequiresUserGesture = false
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
        // Launch routing (mirrors iOS ContentView.boot):
        //  - a remote deep link → add it and connect (start core lazily);
        //  - saved servers, no deep link → the native picker;
        //  - otherwise → local play as before.
        when {
            remoteDeepLink != null -> {
                if (intent?.data?.getQueryParameter("save") != "0") {
                    RemoteStore.add(this, remoteDeepLink)
                }
                startCoreThen { showRemote(remoteDeepLink) }
            }
            RemoteStore.list(this).isNotEmpty() && lichFragment == null -> showPicker()
            else -> bootAndLoad()
        }
    }

    /** Boot the core and load local play (used when no servers are saved). */
    private fun bootAndLoad() {
        startCoreThen { showLocal() }
    }

    /**
     * Ensure the embedded core is up (idempotent), then run [onReady] on the
     * UI thread. Reused by local play and by connecting to a remote server
     * (the core keeps running idle in remote mode). Shows the WebView when
     * starting the core so a picker view isn't left on screen.
     */
    private fun startCoreThen(onReady: () -> Unit) {
        if (coreStarted && bootPort > 0 && bootToken != null) {
            runOnUiThread { onReady() }
            return
        }
        runOnUiThread { showWebView() }
        Thread({
            CryptoKeys.installPasswordKey(this)
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
                coreStarted = true
                onReady()
            }
        }, "core-boot").start()
    }

    private fun bootUrl(port: Int, token: String): String {
        // app=1 marks the shell for the web client. nativepicker=1 tells it a
        // native character picker owns remote-server management, so the
        // in-page Remote login tab is hidden.
        var url = "http://127.0.0.1:$port/play#token=$token&app=1&nativepicker=1"
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
            runOnUiThread {
                showWebView()
                webView.loadUrl(bootUrl(port, token))
            }
        }
    }

    /** Make the WebView the current content view (leaving the picker). */
    private fun showWebView() {
        showingPicker = false
        if (webView.parent == null) {
            setContentView(webView)
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
        // nativepicker=1: hide the web client's in-page Remote tab; the native
        // picker (reachable via "Switch character") owns switching servers.
        val fragment = if (target.token.isEmpty()) {
            "app=1&nativepicker=1"
        } else {
            "token=${target.token}&app=1&nativepicker=1"
        }
        runOnUiThread {
            showWebView()
            webView.loadUrl("http://$host:${target.port}/#$fragment")
        }
    }

    /** Show the native character picker (launch, and "Switch character"). */
    private fun showPicker() {
        val view = RemotePickerView(this, object : RemotePickerView.Callbacks {
            override fun onPlayLocal() = startCoreThen { showLocal() }
            override fun onConnect(target: RemoteStore.Target) = startCoreThen { showRemote(target) }
            override fun onScanQr() = launchScanner()
            override fun onAddManual(target: RemoteStore.Target) {
                RemoteStore.add(this@MainActivity, target)
                picker?.refresh()
            }
            override fun onDelete(id: String) {
                RemoteStore.remove(this@MainActivity, id)
                picker?.refresh()
            }
        })
        picker = view
        showingPicker = true
        setContentView(view)
    }

    private fun launchScanner() {
        startActivityForResult(Intent(this, QrScannerActivity::class.java), SCAN_REQUEST)
    }

    @Deprecated("startActivityForResult is fine for a single scanner result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == SCAN_REQUEST && resultCode == RESULT_OK) {
            val text = data?.getStringExtra(QrScannerActivity.RESULT_TEXT) ?: return
            val uri = Uri.parse(text)
            val target = if (uri.scheme == "vellum" && uri.host == "remote") {
                remoteTargetFrom(uri)
            } else {
                null
            } ?: return
            RemoteStore.add(this, target)
            picker?.refresh()
        }
    }

    /** vellum:// navigations from the page itself (settings actions). */
    private fun handleShellUrl(uri: Uri) {
        when (uri.host) {
            "local" -> showLocal()
            "remote" -> when (uri.path.orEmpty()) {
                "", "/" -> {
                    // Pair: vellum://remote?host&port[&token][&name][&save=0].
                    val target = remoteTargetFrom(uri) ?: return
                    if (uri.getQueryParameter("save") != "0") {
                        RemoteStore.add(this, target)
                    }
                    showRemote(target)
                }
                // "Switch character" in the web settings sheet → back to the
                // native picker (which the shell owns).
                "/picker" -> showPicker()
            }
        }
    }

    /** A vellum://remote deep link delivered by the OS (system-camera scan),
     * as a Target; null for any other intent. */
    private fun remoteTargetFromIntent(intent: Intent?): RemoteStore.Target? {
        val uri = intent?.data ?: return null
        if (uri.scheme != "vellum" || uri.host != "remote") return null
        if (uri.path.orEmpty() !in listOf("", "/")) return null
        return remoteTargetFrom(uri)
    }

    private fun remoteTargetFrom(uri: Uri): RemoteStore.Target? {
        val host = uri.getQueryParameter("host")?.trim().orEmpty()
        val port = uri.getQueryParameter("port")?.trim()?.toIntOrNull()
        if (host.isEmpty() || port == null || port !in 1..65535) return null
        // The character name rides the extended .webinfo deep link so a
        // scanned entry auto-names itself; fall back to host:port.
        val name = uri.getQueryParameter("name")?.trim()?.takeIf { it.isNotEmpty() }
            ?: "$host:$port"
        return RemoteStore.Target(
            host = host,
            port = port,
            token = uri.getQueryParameter("token")?.trim().orEmpty(),
            name = name,
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

    /** singleTask: a deep link while running lands here instead of a fresh
     * activity.
     *  - vellum://remote?… → add the character and connect (native picker
     *    owns remote servers now);
     *  - vellum://lich?… → prefill the web Lich tab, back to local play. */
    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        remoteTargetFromIntent(intent)?.let { target ->
            if (intent?.data?.getQueryParameter("save") != "0") {
                RemoteStore.add(this, target)
            }
            startCoreThen { showRemote(target) }
            return
        }
        lichFragmentFrom(intent)?.let {
            lichFragment = it
            startCoreThen { showLocal() }
        }
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
        private const val SCAN_REQUEST = 7
    }

    @Deprecated("Deprecated in API 33; fine with legacy back handling")
    override fun onBackPressed() {
        // On the picker, Back backgrounds the app (the picker is the root).
        if (showingPicker) {
            moveTaskToBack(true)
            return
        }
        // In the WebView: navigate it; at its root, return to the picker when
        // servers are saved (so remote users land back on the character list),
        // otherwise background the app.
        if (webView.canGoBack()) {
            webView.goBack()
        } else if (RemoteStore.list(this).isNotEmpty()) {
            showPicker()
        } else {
            moveTaskToBack(true)
        }
    }
}
