package dev.vellumfe

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import dev.vellumfe.core.VellumCore
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/**
 * Foreground service that owns the Rust core, so the game connection
 * survives the screen turning off and the activity being backgrounded.
 *
 * Lifecycle guardrails (playtest-driven): the service polls the core's
 * /status endpoint and
 *  - holds the wakelock only while the session is active
 *    (authenticating/connecting/connected/reconnecting), releasing it at
 *    the login screen so an idle core doesn't drain the battery;
 *  - stops itself entirely when the user swiped the app away AND the
 *    session is no longer active — swiping away mid-session intentionally
 *    keeps playing, but nobody wants a zombie service behind a login
 *    screen they can't see.
 * The core itself also gives up reconnecting after repeated unattended
 * losses, so a forgotten phone converges to "service stopped" instead of
 * relogging all night.
 *
 * Declared foregroundServiceType="specialUse": a persistent game session
 * has no natural end, and Android 15 caps dataSync services at 6 hours.
 */
class CoreService : Service() {

    private var wakeLock: PowerManager.WakeLock? = null
    @Volatile private var statusUrl: String? = null
    @Volatile private var taskRemoved = false
    @Volatile private var stopping = false
    private var pollThread: Thread? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        val notification = buildNotification("Starting…")
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }

        wakeLock = (getSystemService(POWER_SERVICE) as PowerManager)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vellumfe:core")
            .apply { setReferenceCounted(false) }

        pollThread = Thread({
            // Password key first: the core seals saved passwords with it.
            CryptoKeys.installPasswordKey(this)
            // JNI boot (config load + server bind), then the status loop.
            val info = JSONObject(VellumCore.startCore(filesDir.absolutePath))
            if (info.has("error")) {
                Log.e(TAG, "core start failed: ${info.optString("error")}")
                updateNotification("Core failed to start")
                return@Thread
            }
            statusUrl =
                "http://127.0.0.1:${info.getInt("port")}/status?token=${info.getString("token")}"
            while (!stopping) {
                applyStatus(fetchState())
                try {
                    Thread.sleep(POLL_INTERVAL_MS)
                } catch (_: InterruptedException) {
                    // stop requested or task removed: re-check immediately
                }
            }
        }, "core-status")
        pollThread?.start()
    }

    private fun fetchState(): String {
        val url = statusUrl ?: return "unknown"
        return try {
            val conn = URL(url).openConnection() as HttpURLConnection
            conn.connectTimeout = 1000
            conn.readTimeout = 1000
            conn.inputStream.bufferedReader().use { reader ->
                JSONObject(reader.readText()).optString("state", "unknown")
            }
        } catch (e: Exception) {
            Log.w(TAG, "status poll failed: $e")
            "unknown"
        }
    }

    private fun applyStatus(state: String) {
        val active = state in ACTIVE_STATES
        if (active) {
            if (wakeLock?.isHeld != true) wakeLock?.acquire()
        } else {
            if (wakeLock?.isHeld == true) wakeLock?.release()
        }
        updateNotification(
            when (state) {
                "connected" -> "Playing — session live"
                "reconnecting" -> "Reconnecting…"
                "authenticating", "connecting" -> "Logging in…"
                "idle" -> "At the login screen"
                "disconnected" -> "Session ended"
                else -> "Running"
            },
        )
        if (taskRemoved && !active && !stopping) {
            Log.i(TAG, "app swiped away and session $state — stopping service")
            stopping = true
            stopSelf()
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopping = true
            stopSelf()
            return START_NOT_STICKY
        }
        // The activity came (back) to life: the task exists again.
        taskRemoved = false
        return START_STICKY
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        // User swiped the app away. Mid-session that's "keep playing";
        // at the login screen it means "I'm done" — the poll decides.
        taskRemoved = true
        pollThread?.interrupt()
        super.onTaskRemoved(rootIntent)
    }

    override fun onDestroy() {
        stopping = true
        pollThread?.interrupt()
        if (wakeLock?.isHeld == true) wakeLock?.release()
        wakeLock = null
        // stopCore joins the runtime thread; keep it off the main thread
        // to avoid an ANR during teardown.
        Thread({ VellumCore.stopCore() }, "core-stop").start()
        super.onDestroy()
    }

    private fun createChannel() {
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Game session",
            NotificationManager.IMPORTANCE_LOW, // silent, no heads-up
        ).apply { description = "Keeps the GemStone IV connection alive" }
        (getSystemService(NOTIFICATION_SERVICE) as NotificationManager)
            .createNotificationChannel(channel)
    }

    private fun updateNotification(text: String) {
        (getSystemService(NOTIFICATION_SERVICE) as NotificationManager)
            .notify(NOTIFICATION_ID, buildNotification(text))
    }

    private fun buildNotification(text: String): Notification {
        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val stop = PendingIntent.getService(
            this,
            1,
            Intent(this, CoreService::class.java).setAction(ACTION_STOP),
            PendingIntent.FLAG_IMMUTABLE,
        )
        return Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_launcher_foreground)
            .setContentTitle("VellumFE")
            .setContentText(text)
            .setContentIntent(openApp)
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null, "Stop", stop).build())
            .build()
    }

    companion object {
        const val CHANNEL_ID = "vellum-core"
        const val NOTIFICATION_ID = 1
        const val ACTION_STOP = "dev.vellumfe.STOP"
        const val POLL_INTERVAL_MS = 30_000L
        private const val TAG = "VellumShell"
        private val ACTIVE_STATES =
            setOf("authenticating", "connecting", "connected", "reconnecting")
    }
}
