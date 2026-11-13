package dev.meteo.app

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.ShortcutInfo
import android.content.pm.ShortcutManager
import android.graphics.drawable.Icon
import dev.meteo.app.widget.RustBridge
import org.json.JSONArray

/** Launcher shortcuts (long-press the icon): one per saved city. */
object Shortcuts {
    /** Intent extra: the city's key (meteo_core::Location::key). */
    const val EXTRA_CITY = "dev.meteo.app.CITY"

    fun update(context: Context) {
        val manager = context.getSystemService(ShortcutManager::class.java) ?: return
        val cities = JSONArray(RustBridge.cities(context.filesDir.absolutePath))
        val max = manager.maxShortcutCountPerActivity.coerceAtMost(4)
        val shortcuts = (0 until minOf(cities.length(), max)).map { i ->
            val city = cities.getJSONObject(i)
            val key = city.getString("key")
            val name = city.getString("name")
            val detail = city.optString("detail")
            val intent = Intent(Intent.ACTION_VIEW)
                .setComponent(ComponentName(context, ShortcutActivity::class.java))
                .putExtra(EXTRA_CITY, key)
            ShortcutInfo.Builder(context, "city-$key")
                .setShortLabel(if (city.optBoolean("device")) "📍 $name" else name)
                .setLongLabel(if (detail.isEmpty()) name else "$name, $detail")
                .setIcon(Icon.createWithResource(context, R.mipmap.ic_launcher))
                .setIntent(intent)
                .setRank(i)
                .build()
        }
        manager.dynamicShortcuts = shortcuts
    }
}
