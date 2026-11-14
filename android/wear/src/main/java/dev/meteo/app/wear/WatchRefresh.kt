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
 * the tile to redraw. After a failure (offline), it waits before trying
 * again: 1 min, then 2, 4… up to 30 min.
 */
object WatchRefresh {
    private val running = AtomicBoolean(false)
    @Volatile private var failures = 0
    @Volatile private var lastFailure = 0L

    /** [force]: the user opened the app, so try now even after a failure. */
    fun start(context: Context, force: Boolean = false, then: (() -> Unit)? = null) {
        val wait = minOf(60_000L shl minOf(failures, 5), 30 * 60_000L)
        if (!force && failures > 0 && System.currentTimeMillis() - lastFailure < wait) {
            then?.invoke()
            return
        }
        if (!running.compareAndSet(false, true)) return
        val app = context.applicationContext
        thread {
            try {
                val dir = app.filesDir.absolutePath
                lastPosition(app)?.let { (lat, lon) ->
                    RustBridge.setDevice(dir, placeName(app, lat, lon), lat, lon)
                }
                if (RustBridge.refresh(dir)) {
                    failures = 0
                    // Only redraw with new data: a redraw of a stale forecast
                    // would start another refresh straight away.
                    TileService.getUpdater(app).requestUpdate(WeatherTileService::class.java)
                } else {
                    failures++
                    lastFailure = System.currentTimeMillis()
                }
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
