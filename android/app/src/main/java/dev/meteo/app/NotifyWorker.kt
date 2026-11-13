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
    }

    override suspend fun doWork(): Result {
        val dir = applicationContext.filesDir.absolutePath
        if (!RustBridge.notificationsEnabled(dir)) return Result.success()
        RustBridge.refresh(dir)
        MeteoWidget().updateAll(applicationContext)

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
            val notification = NotificationCompat.Builder(applicationContext, kind)
                .setSmallIcon(R.drawable.ic_launcher_monochrome)
                .setContentTitle(n.getString("title"))
                .setContentText(n.getString("text"))
                .setStyle(NotificationCompat.BigTextStyle().bigText(n.getString("text")))
                .setContentIntent(openApp)
                .setAutoCancel(true)
                .build()
            NotificationManagerCompat.from(applicationContext).notify(if (kind == CHANNEL_RAIN) 2 else 1, notification)
        }
        return Result.success()
    }
}
