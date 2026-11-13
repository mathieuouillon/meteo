package dev.meteo.app

import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

/**
 * Starts with the app's process (the Slint activity, the widget or the
 * worker): creates the notification channels and keeps the background
 * check scheduled. The check itself does nothing unless a notification is
 * turned on in the app's settings.
 */
class MeteoApp : Application() {
    companion object {
        /** Runs the check now (called from Rust when a notification is turned on). */
        @JvmStatic
        fun checkNow(context: Context) {
            val request = OneTimeWorkRequestBuilder<NotifyWorker>()
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork("meteo-notify-now", ExistingWorkPolicy.REPLACE, request)
        }
    }

    override fun onCreate() {
        super.onCreate()
        val fr = resources.configuration.locales[0].language == "fr"
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(
                NotifyWorker.CHANNEL_MORNING,
                if (fr) "Résumé du matin" else "Morning summary",
                NotificationManager.IMPORTANCE_DEFAULT,
            ),
        )
        manager.createNotificationChannel(
            NotificationChannel(
                NotifyWorker.CHANNEL_RAIN,
                if (fr) "Alertes pluie" else "Rain alerts",
                NotificationManager.IMPORTANCE_HIGH,
            ),
        )
        val request = PeriodicWorkRequestBuilder<NotifyWorker>(15, TimeUnit.MINUTES)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(this)
            .enqueueUniquePeriodicWork("meteo-notify", ExistingPeriodicWorkPolicy.KEEP, request)
    }
}
