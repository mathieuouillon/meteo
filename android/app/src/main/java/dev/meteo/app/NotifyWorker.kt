package dev.meteo.app

import android.Manifest
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.glance.appwidget.updateAll
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import dev.meteo.app.widget.MeteoWidget
import dev.meteo.app.widget.RainWidget
import dev.meteo.app.widget.RoundWidget
import dev.meteo.app.widget.RustBridge
import org.json.JSONArray

/**
 * Every 15 minutes (when notifications are on): refreshes the forecast in
 * Rust, asks Rust which notifications are due (meteo_core::notify) and posts
 * them. Rust remembers what was sent, so nothing repeats.
 */
class NotifyWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    companion object {
        const val CHANNEL_MORNING = "morning"
        const val CHANNEL_RAIN = "rain"
        const val CHANNEL_WARNING = "warning"
        const val CHANNEL_TEMPERATURE = "temperature"
    }

    override suspend fun doWork(): Result {
        val dir = applicationContext.filesDir.absolutePath
        if (!RustBridge.notificationsEnabled(dir)) return Result.success()
        RustBridge.refresh(dir)
        MeteoWidget().updateAll(applicationContext)
        RainWidget().updateAll(applicationContext)
        RoundWidget().updateAll(applicationContext)

        val allowed = ContextCompat.checkSelfPermission(applicationContext, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
        if (!allowed) return Result.success()
        val language = applicationContext.resources.configuration.locales[0].language
        val due = JSONArray(RustBridge.notifications(dir, language))
        val openApp = PendingIntent.getActivity(
            applicationContext, 0,
            Intent().setComponent(ComponentName(applicationContext.packageName, "android.app.NativeActivity")),
            PendingIntent.FLAG_IMMUTABLE,
        )
        for (i in 0 until due.length()) {
            val n = due.getJSONObject(i)
            val kind = n.getString("kind")
            val builder = NotificationCompat.Builder(applicationContext, kind)
                .setSmallIcon(R.drawable.ic_launcher_monochrome)
                .setContentTitle(n.getString("title"))
                .setContentText(n.getString("text"))
                .setStyle(NotificationCompat.BigTextStyle().bigText(n.getString("text")))
                .setContentIntent(openApp)
                .setAutoCancel(true)
            // Rain soon: a live countdown to the start (the system updates it,
            // the app isn't woken), gone 15 minutes after the rain starts.
            if (n.has("starts_at")) {
                val startsAt = n.getLong("starts_at") * 1000
                builder.setWhen(startsAt)
                    .setShowWhen(true)
                    .setUsesChronometer(true)
                    .setChronometerCountDown(true)
                    .setTimeoutAfter(maxOf(startsAt + 15 * 60_000 - System.currentTimeMillis(), 60_000))
            }
            val notification = builder.build()
            NotificationManagerCompat.from(applicationContext).notify(
                when (kind) {
                    CHANNEL_RAIN -> 2
                    CHANNEL_TEMPERATURE -> 3
                    // Each warning keeps its own notification.
                    CHANNEL_WARNING -> 100 + (n.getString("text").hashCode() and 0xffff)
                    else -> 1
                },
                notification,
            )
        }
        return Result.success()
    }
}
