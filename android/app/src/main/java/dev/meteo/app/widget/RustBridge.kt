package dev.meteo.app.widget

import org.json.JSONObject

/** Calls into libmeteo_widget.so (Rust, widget-ffi crate). */
object RustBridge {
    init {
        System.loadLibrary("meteo_widget")
    }

    /** The widget's content for the selected city, as JSON (meteo_core::WidgetState). */
    external fun state(dataDir: String): String

    /** Downloads the selected city's forecast into the cache. Blocking. */
    external fun refresh(dataDir: String): Boolean
}

data class WidgetHour(val label: String, val icon: String, val temperature: String)

data class WidgetView(
    val city: String,
    val isDevice: Boolean,
    val icon: String,
    val temperature: String,
    val condition: String,
    val highLow: String,
    val updated: String,
    val skyTop: Int,
    val skyBottom: Int,
    val hours: List<WidgetHour>,
)

data class WidgetState(val needsRefresh: Boolean, val view: WidgetView?)

fun parseWidgetState(json: String): WidgetState {
    val o = JSONObject(json)
    val v = o.optJSONObject("view")
    val view = v?.let {
        val hours = it.getJSONArray("hours")
        WidgetView(
            city = it.getString("city"),
            isDevice = it.getBoolean("is_device"),
            icon = it.getString("icon"),
            temperature = it.getString("temperature"),
            condition = it.getString("condition"),
            highLow = it.getString("high_low"),
            updated = it.getString("updated"),
            skyTop = it.getInt("sky_top") or 0xFF000000.toInt(),
            skyBottom = it.getInt("sky_bottom") or 0xFF000000.toInt(),
            hours = (0 until hours.length()).map { i ->
                val h = hours.getJSONObject(i)
                WidgetHour(h.getString("label"), h.getString("icon"), h.getString("temperature"))
            },
        )
    }
    return WidgetState(o.getBoolean("needs_refresh"), view)
}
