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

    /** Boot progress + latest requested destination (UI thread only).
     * Boot state and navigation are tracked separately so the newest
     * request always wins, however background boots complete. */
    private val nav = BootNavState()

    /** Fragment tail from a vellum://lich deep link; rides the boot URL so
     * the web client prefills the Lich login tab. (Remote deep links no
     * longer prefill the page — the native picker owns remote servers.) */
    private var lichFragment: String? = null

    /** Remote host the WebView may browse in-app (Remote mode); null while
     * on the embedded core. Everything else non-loopback goes external. */
    private var allowedRemoteHost: String? = null

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
        //  - otherwise → local play. The login page is the app's front door;
        //    the native character picker (saved remote servers, Scan QR, Add
        //    manually) is reached from the in-page Characters button
        //    (vellum://remote/picker), not shown at launch.
        when {
            remoteDeepLink != null -> {
                if (intent?.data?.getQueryParameter("save") != "0") {
                    RemoteStore.add(this, remoteDeepLink)
                }
                navigate(NavDestination.Remote(remoteDeepLink))
            }
            else -> navigate(NavDestination.Local)
        }
    }

    /**
     * Record a navigation request (UI thread) and act on it. The request is
     * stored immediately; if the core is ready the destination renders now,
     * otherwise a single boot thread is (re)used and, on completion, renders
     * whatever destination is latest at that moment — never the one current
     * when the boot began.
     */
    private fun navigate(dest: NavDestination) {
        when (val effect = nav.onRequest(dest)) {
            is BootNavState.Effect.Render -> render(effect.dest)
            BootNavState.Effect.StartBoot -> {
                // Show the WebView while booting so a picker view isn't
                // left on screen; the boot URL loads when the core is up.
                showWebView()
                startBootThread()
            }
            BootNavState.Effect.None -> {
                // Coalesced into the boot already in flight (or the
                // activity is being destroyed); nothing to do now.
            }
        }
    }

    /** Show a destination now (core ready, or picker which needs none). */
    private fun render(dest: NavDestination) {
        when (dest) {
            is NavDestination.Local -> showLocal()
            is NavDestination.Remote -> showRemote(dest.target)
            is NavDestination.Picker -> showPicker()
        }
    }

    /**
     * The single in-flight core boot (idempotent at the Rust layer; the
     * service's independent start is unaffected). Publishes its result via
     * [nav] on the UI thread; a destroyed activity has invalidated [nav],
     * so no late UI work runs.
     */
    private fun startBootThread() {
        Thread({
            CryptoKeys.installPasswordKey(this)
            val info = JSONObject(VellumCore.startCore(filesDir.absolutePath))
            if (info.has("error")) {
                runOnUiThread {
                    if (nav.onBootFailure()) {
                        showError("Core failed to start:\n${info.optString("error")}")
                    }
                }
                return@Thread
            }
            val port = info.getInt("port")
            val token = info.getString("token")
            if (!waitForServer(port)) {
                runOnUiThread {
                    if (nav.onBootFailure()) {
                        showError("The embedded server did not come up on port $port.")
                    }
                }
                return@Thread
            }
            runOnUiThread {
                val effect = nav.onBootSuccess(port, token)
                if (effect is BootNavState.Effect.Render) {
                    render(effect.dest)
                }
            }
        }, "core-boot").start()
    }

    private fun bootUrl(port: Int, token: String): String {
        // app=1 marks the shell for the web client. nativepicker=1 tells it a
        // native character picker owns remote-server management, so the
        // in-page Remote login tab is hidden.
        var url = "http://127.0.0.1:$port/play#token=$token&app=1&nativepicker=1"
        charsFragment()?.let { url += "&$it" }
        lichFragment?.let { url += "&$it" }
        return url
    }

    /** The saved characters as `chars=` + `charids=` fragment params for
     * the web client's switch-character wheel: display labels plus each
     * target's stable store ID (same order). A wheel pick round-trips
     * through vellum://remote/connect?id=… and is resolved BY ID — names
     * are display only, so duplicate labels stay distinct. Pairing tokens
     * stay in native storage; this shell connects with its own stored
     * token. Null when nothing is saved. */
    private fun charsFragment(): String? = CharacterWheel.fragment(RemoteStore.list(this))

    /** Reload the local boot URL (embedded login page). Only called via
     * [render] once the core is ready, so port/token are published. */
    private fun showLocal() {
        allowedRemoteHost = null
        val port = nav.port
        val token = nav.token
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
        var fragment = if (target.token.isEmpty()) {
            "app=1&nativepicker=1"
        } else {
            "token=${target.token}&app=1&nativepicker=1"
        }
        charsFragment()?.let { fragment += "&$it" }
        runOnUiThread {
            showWebView()
            webView.loadUrl("http://$host:${target.port}/#$fragment")
        }
    }

    /** Show the native character picker (launch, and "Switch character"). */
    private fun showPicker() {
        val view = RemotePickerView(this, object : RemotePickerView.Callbacks {
            override fun onPlayLocal() = navigate(NavDestination.Local)
            override fun onConnect(target: RemoteStore.Target) = navigate(NavDestination.Remote(target))
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
            "local" -> navigate(NavDestination.Local)
            "remote" -> when (uri.path.orEmpty()) {
                "", "/" -> {
                    // Pair: vellum://remote?host&port[&token][&name][&save=0].
                    val target = remoteTargetFrom(uri) ?: return
                    if (uri.getQueryParameter("save") != "0") {
                        RemoteStore.add(this, target)
                    }
                    navigate(NavDestination.Remote(target))
                }
                // "Switch character" in the web settings sheet → back to the
                // native picker (which the shell owns).
                "/picker" -> navigate(NavDestination.Picker)
                // Switch-character wheel pick: connect to a saved server by
                // its stable store ID (the token comes from native storage,
                // never the page). Legacy name-only requests resolve only
                // when exactly one entry matches; anything unknown,
                // deleted, or ambiguous lands on the picker — never on a
                // different entry.
                "/connect" -> {
                    val target = CharacterWheel.resolve(
                        RemoteStore.list(this),
                        uri.getQueryParameter("id"),
                        uri.getQueryParameter("name"),
                    )
                    if (target != null) {
                        navigate(NavDestination.Remote(target))
                    } else {
                        navigate(NavDestination.Picker)
                    }
                }
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
            navigate(NavDestination.Remote(target))
            return
        }
        lichFragmentFrom(intent)?.let {
            lichFragment = it
            navigate(NavDestination.Local)
        }
    }

    override fun onDestroy() {
        // Drop pending UI work from any in-flight boot; late completions
        // become no-ops. The service-owned core is deliberately untouched —
        // activity recreation must never tear down a live session.
        nav.invalidate()
        super.onDestroy()
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
        // In the WebView: navigate it. At its root: a remote view returns to
        // the picker (back to the character list); local play is the app's
        // root now — the login page is the front door — so Back backgrounds
        // the app instead of surfacing the picker.
        if (webView.canGoBack()) {
            webView.goBack()
        } else if (allowedRemoteHost != null) {
            showPicker()
        } else {
            moveTaskToBack(true)
        }
    }
}
