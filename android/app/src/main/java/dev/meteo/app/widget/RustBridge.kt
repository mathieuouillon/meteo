package dev.meteo.app.widget

import org.json.JSONObject

/** Calls into libmeteo_widget.so (Rust, widget-ffi crate). */
object RustBridge {
    init {
        System.loadLibrary("meteo_widget")
    }

    /** The widget's content for the selected city, as JSON (meteo_core::WidgetState),
     *  in `language` ("fr", "en", …). */
    external fun state(dataDir: String, language: String): String

    /** Downloads the selected city's forecast into the cache. Blocking. */
    external fun refresh(dataDir: String): Boolean

    /** Whether a notification (morning summary or rain alert) is turned on. */
    external fun notificationsEnabled(dataDir: String): Boolean

    /** The notifications due now, as JSON [{kind, title, text}], in `language`. */
    external fun notifications(dataDir: String, language: String): String

    /** The saved cities, as JSON [{key, name, detail, device}]. */
    external fun cities(dataDir: String): String

    /** Selects the saved city with that key; false if there's none. */
    external fun selectCity(dataDir: String, key: String): Boolean
}

data class WidgetHour(val label: String, val iconKind: String, val temperature: String)

/** The widget's content; colours are the app's weather palette (0xRRGGBB). */
data class WidgetView(
    val city: String,
    val isDevice: Boolean,
    val iconKind: String,
    val temperature: String,
    val condition: String,
    val highLow: String,
    val feelsLike: String,
    val updated: String,
    val dark: Boolean,
    val top: Int,
    val bottom: Int,
    val card: Int,
    val ink: Int,
    val inkSoft: Int,
    val accent: Int,
    val hours: List<WidgetHour>,
    val nowcastTitle: String,
    val nowcastSummary: String,
    /** 15-minute bars: height 0–1, and whether it rains. */
    val nowcast: List<Pair<Double, Boolean>>,
)

data class WidgetState(val needsRefresh: Boolean, val view: WidgetView?)

fun parseWidgetState(json: String): WidgetState {
    val o = JSONObject(json)
    val v = o.optJSONObject("view")
    val opaque = { key: String -> v!!.getInt(key) or 0xFF000000.toInt() }
    val view = v?.let {
        val hours = it.getJSONArray("hours")
        WidgetView(
            city = it.getString("city"),
            isDevice = it.getBoolean("is_device"),
            iconKind = it.getString("icon_kind"),
            temperature = it.getString("temperature"),
            condition = it.getString("condition"),
            highLow = it.getString("high_low"),
            feelsLike = it.getString("feels_like"),
            updated = it.getString("updated"),
            dark = it.getBoolean("dark"),
            top = opaque("top"),
            bottom = opaque("bottom"),
            card = opaque("card"),
            ink = opaque("ink"),
            inkSoft = opaque("ink_soft"),
            accent = opaque("accent"),
            hours = (0 until hours.length()).map { i ->
                val h = hours.getJSONObject(i)
                WidgetHour(h.getString("label"), h.getString("icon_kind"), h.getString("temperature"))
            },
            nowcastTitle = it.optString("nowcast_title"),
            nowcastSummary = it.optString("nowcast_summary"),
            nowcast = it.optJSONArray("nowcast")?.let { bars ->
                (0 until bars.length()).map { i -> bars.getJSONArray(i).let { b -> b.getDouble(0) to b.getBoolean(1) } }
            } ?: emptyList(),
        )
    }
    return WidgetState(o.getBoolean("needs_refresh"), view)
}
