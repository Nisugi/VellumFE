package dev.vellumfe

import android.content.Context
import android.graphics.Color
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.app.AlertDialog
import java.net.HttpURLConnection
import java.net.URL

/**
 * The launch character picker (native): saved remote VellumFE servers plus
 * "play on this phone". Add by scanning a `.webinfo` QR or entering
 * host/port/token manually; delete per row; each row shows a live/offline
 * dot from a `/health` probe. Mirrors iOS `RemotePickerView.swift`.
 *
 * Built as a programmatic View (this shell has no Compose / XML-layout
 * dependency); the host activity swaps it in via setContentView, exactly as
 * the iOS shell swaps its `phase`.
 */
class RemotePickerView(
    context: Context,
    private val callbacks: Callbacks,
) : ScrollView(context) {

    interface Callbacks {
        fun onPlayLocal()
        fun onConnect(target: RemoteStore.Target)
        fun onScanQr()
        fun onAddManual(target: RemoteStore.Target)
        fun onDelete(id: String)
    }

    private val column = LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(dp(20), dp(24), dp(20), dp(24))
    }

    /** id → reachable (null = probe not finished). */
    private val health = HashMap<String, Boolean?>()

    init {
        setBackgroundColor(BG)
        addView(column)
        render()
        probeAll()
    }

    /** Rebuild the list (called after add/delete). */
    fun refresh() {
        render()
        probeAll()
    }

    private fun render() {
        column.removeAllViews()

        column.addView(heading("Characters"))

        val servers = RemoteStore.list(context)
        if (servers.isEmpty()) {
            column.addView(subtle("No saved characters yet. Scan a pairing QR or add one manually."))
        }
        for (server in servers) {
            column.addView(serverRow(server))
        }

        column.addView(spacer(dp(16)))
        column.addView(actionButton("▶  Play on this phone") { callbacks.onPlayLocal() })
        column.addView(actionButton("⧉  Scan QR to add") { callbacks.onScanQr() })
        column.addView(actionButton("＋  Add manually") { showManualDialog() })
    }

    private fun serverRow(server: RemoteStore.Target): View {
        val row = LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(12), dp(12), dp(12), dp(12))
            background = pill()
            isClickable = true
            setOnClickListener { callbacks.onConnect(server) }
        }

        val dot = TextView(context).apply {
            text = "●"
            setTextColor(statusColor(server.id))
            textSize = 14f
            setPadding(0, 0, dp(10), 0)
        }
        row.addView(dot)

        val texts = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, wrap, 1f)
        }
        texts.addView(TextView(context).apply {
            text = server.name
            setTextColor(Color.parseColor("#E6E6E6"))
            textSize = 16f
        })
        texts.addView(TextView(context).apply {
            text = "${server.host}:${server.port}"
            setTextColor(Color.parseColor("#8A8A8A"))
            textSize = 12f
            typeface = android.graphics.Typeface.MONOSPACE
        })
        row.addView(texts)

        row.addView(TextView(context).apply {
            text = statusText(server.id)
            setTextColor(Color.parseColor("#8A8A8A"))
            textSize = 11f
            setPadding(dp(8), 0, dp(8), 0)
        })

        val del = Button(context).apply {
            text = "✕"
            setTextColor(Color.parseColor("#C98A8A"))
            setBackgroundColor(Color.TRANSPARENT)
            setOnClickListener {
                AlertDialog.Builder(context)
                    .setTitle("Delete ${server.name}?")
                    .setMessage("Removes the saved server (and its pairing token).")
                    .setPositiveButton("Delete") { _, _ -> callbacks.onDelete(server.id) }
                    .setNegativeButton("Cancel", null)
                    .show()
            }
        }
        row.addView(del)

        val wrapper = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(match, wrap).apply {
                topMargin = dp(8)
            }
            addView(row)
        }
        return wrapper
    }

    private fun showManualDialog() {
        val pad = dp(16)
        val form = LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(pad, pad, pad, 0)
        }
        val nameIn = field("Name (e.g. Rysk)", InputType.TYPE_CLASS_TEXT)
        val hostIn = field("Host (e.g. 192.168.1.21)", InputType.TYPE_TEXT_VARIATION_URI)
        val portIn = field("Port (e.g. 8042)", InputType.TYPE_CLASS_NUMBER)
        val tokenIn = field("Pairing token (optional)", InputType.TYPE_CLASS_TEXT)
        form.addView(nameIn); form.addView(hostIn); form.addView(portIn); form.addView(tokenIn)

        AlertDialog.Builder(context)
            .setTitle("Add character")
            .setView(form)
            .setPositiveButton("Save") { _, _ ->
                val host = hostIn.text.toString().trim()
                val port = portIn.text.toString().trim().toIntOrNull()
                if (host.isEmpty() || port == null || port !in 1..65535) return@setPositiveButton
                val label = nameIn.text.toString().trim()
                callbacks.onAddManual(
                    RemoteStore.Target(
                        host = host,
                        port = port,
                        token = tokenIn.text.toString().trim(),
                        name = if (label.isEmpty()) "$host:$port" else label,
                    )
                )
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    // ---- Health probes ----------------------------------------------------

    /**
     * GET http://host:port/health per saved server on a background thread;
     * the endpoint is CORS-open and token-free (server.rs), so a 200 means
     * the instance is up. Repaints the list as results land.
     */
    private fun probeAll() {
        val servers = RemoteStore.list(context)
        for (server in servers) {
            Thread({
                val ok = isReachable(server)
                post {
                    health[server.id] = ok
                    render()
                }
            }, "probe-${server.port}").start()
        }
    }

    private fun isReachable(server: RemoteStore.Target): Boolean {
        val host = if (server.host.contains(":") && !server.host.startsWith("[")) {
            "[${server.host}]"
        } else {
            server.host
        }
        return try {
            val conn = URL("http://$host:${server.port}/health")
                .openConnection() as HttpURLConnection
            conn.connectTimeout = 2000
            conn.readTimeout = 2000
            conn.responseCode == 200
        } catch (_: Exception) {
            false
        }
    }

    private fun statusColor(id: String): Int = when (health[id]) {
        true -> Color.parseColor("#4CAF50")
        false -> Color.parseColor("#666666")
        null -> Color.parseColor("#666666")
    }

    private fun statusText(id: String): String = when (health[id]) {
        true -> "live"
        false -> "offline"
        null -> "…"
    }

    // ---- View helpers -----------------------------------------------------

    private fun heading(text: String) = TextView(context).apply {
        this.text = text
        setTextColor(ACCENT)
        textSize = 22f
        setPadding(0, 0, 0, dp(12))
    }

    private fun subtle(text: String) = TextView(context).apply {
        this.text = text
        setTextColor(Color.parseColor("#8A8A8A"))
        textSize = 13f
        setPadding(0, dp(8), 0, dp(8))
    }

    private fun actionButton(text: String, onClick: () -> Unit) = Button(context).apply {
        this.text = text
        setTextColor(Color.parseColor("#E6E6E6"))
        background = pill()
        gravity = Gravity.START or Gravity.CENTER_VERTICAL
        layoutParams = LinearLayout.LayoutParams(match, wrap).apply { topMargin = dp(8) }
        setOnClickListener { onClick() }
    }

    private fun field(hint: String, type: Int) = EditText(context).apply {
        this.hint = hint
        inputType = type
        setTextColor(Color.parseColor("#E6E6E6"))
        setHintTextColor(Color.parseColor("#8A8A8A"))
    }

    private fun spacer(h: Int) = View(context).apply {
        layoutParams = LinearLayout.LayoutParams(match, h)
    }

    private fun pill(): android.graphics.drawable.GradientDrawable =
        android.graphics.drawable.GradientDrawable().apply {
            setColor(Color.parseColor("#1B1E24"))
            cornerRadius = dp(10).toFloat()
        }

    private fun dp(v: Int): Int = TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics
    ).toInt()

    companion object {
        private val BG = Color.parseColor("#111318")
        private val ACCENT = Color.parseColor("#E5C07B")
        private const val match = LinearLayout.LayoutParams.MATCH_PARENT
        private const val wrap = LinearLayout.LayoutParams.WRAP_CONTENT
    }
}
