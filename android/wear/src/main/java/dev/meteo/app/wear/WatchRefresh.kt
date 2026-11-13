package dev.meteo.app.wear

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Geocoder
import android.location.LocationManager
import androidx.wear.tiles.TileService
import dev.meteo.app.widget.RustBridge
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

/**
 * Downloads a fresh forecast in the background: for the watch's own
 * approximate position when allowed (otherwise the saved city), then asks
 * the tile to redraw.
 */
object WatchRefresh {
    private val running = AtomicBoolean(false)

    fun start(context: Context, then: (() -> Unit)? = null) {
        if (!running.compareAndSet(false, true)) return
        val app = context.applicationContext
        thread {
            try {
                val dir = app.filesDir.absolutePath
                lastPosition(app)?.let { (lat, lon) ->
                    RustBridge.setDevice(dir, placeName(app, lat, lon), lat, lon)
                }
                RustBridge.refresh(dir)
                TileService.getUpdater(app).requestUpdate(WeatherTileService::class.java)
            } finally {
                running.set(false)
                then?.invoke()
            }
        }
    }

    /** The most recent position any provider knows, if location is allowed. */
    private fun lastPosition(context: Context): Pair<Double, Double>? {
        if (context.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) != PackageManager.PERMISSION_GRANTED) {
            return null
        }
        val manager = context.getSystemService(LocationManager::class.java) ?: return null
        return try {
            listOf("fused", "network", "passive")
                .filter { it in manager.allProviders }
                .mapNotNull { manager.getLastKnownLocation(it) }
                .maxByOrNull { it.time }
                ?.let { it.latitude to it.longitude }
        } catch (_: SecurityException) {
            null
        }
    }

    @Suppress("DEPRECATION")
    private fun placeName(context: Context, lat: Double, lon: Double): String = try {
        Geocoder(context).getFromLocation(lat, lon, 1)?.firstOrNull()?.let { it.locality ?: it.subAdminArea }
    } catch (_: Exception) {
        null
    } ?: if (context.resources.configuration.locales[0].language == "fr") "Ma position" else "My location"
}
