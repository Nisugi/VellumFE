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
import dev.vellumfe.core.VellumCore

/**
 * Foreground service that owns the Rust core, so the game connection
 * survives the screen turning off and the activity being backgrounded.
 *
 * Declared foregroundServiceType="specialUse": a persistent game session
 * has no natural end, and Android 15 caps dataSync services at 6 hours.
 */
class CoreService : Service() {

    private var wakeLock: PowerManager.WakeLock? = null

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }

        // Partial wakelock: the radio and CPU stay up for the TCP session
        // while the screen is off. Held for the service's lifetime in v1;
        // scoping it to connected-only is a later battery refinement.
        wakeLock = (getSystemService(POWER_SERVICE) as PowerManager)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "vellumfe:core")
            .apply {
                setReferenceCounted(false)
                acquire()
            }

        // JNI boot off the main thread (config load + server bind).
        Thread({ VellumCore.startCore(filesDir.absolutePath) }, "core-start").start()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACTION_STOP) {
            stopSelf()
            return START_NOT_STICKY
        }
        return START_STICKY
    }

    override fun onDestroy() {
        wakeLock?.release()
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

    private fun buildNotification(): Notification {
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
            .setContentText("Game session running")
            .setContentIntent(openApp)
            .setOngoing(true)
            .addAction(Notification.Action.Builder(null, "Stop", stop).build())
            .build()
    }

    companion object {
        const val CHANNEL_ID = "vellum-core"
        const val NOTIFICATION_ID = 1
        const val ACTION_STOP = "dev.vellumfe.STOP"
    }
}
