package dev.meteo.app

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.ComponentName
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import dev.meteo.app.widget.RustBridge
import dev.meteo.app.widget.parseWidgetState
import kotlin.concurrent.thread

/**
 * A Quick Settings tile: the selected city's temperature and condition, from
 * the same Rust state as the widgets. Updated each time the panel opens;
 * tapping it opens the app.
 */
class WeatherTileService : TileService() {
    override fun onStartListening() {
        super.onStartListening()
        val dir = filesDir.absolutePath
        val needsRefresh = show(dir)
        // An old or missing forecast: download it, then show it.
        if (needsRefresh) {
            thread {
                if (RustBridge.refresh(dir)) show(dir)
            }
        }
    }

    /** Shows the current state on the tile; true if it needs a refresh. */
    private fun show(dir: String): Boolean {
        val tile = qsTile ?: return false
        val language = resources.configuration.locales[0].language
        val state = parseWidgetState(RustBridge.state(dir, language))
        val view = state.view
        tile.icon = Icon.createWithResource(this, R.drawable.ic_launcher_monochrome)
        tile.state = Tile.STATE_ACTIVE
        if (view != null) {
            tile.label = "${view.temperature} · ${view.city}"
            if (Build.VERSION.SDK_INT >= 29) tile.subtitle = view.condition
            tile.contentDescription = "${view.city}, ${view.temperature}, ${view.condition}"
        } else {
            tile.label = getString(R.string.tile_label)
        }
        tile.updateTile()
        return state.needsRefresh
    }

    @SuppressLint("StartActivityAndCollapseDeprecated")
    override fun onClick() {
        super.onClick()
        val intent = Intent()
            .setComponent(ComponentName(packageName, "android.app.NativeActivity"))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (Build.VERSION.SDK_INT >= 34) {
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }
}
