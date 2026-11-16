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

    /** One notification of each kind, for the settings' test button. */
    external fun testNotifications(dataDir: String, language: String): String

    /** The saved cities, as JSON [{key, name, detail, device}]. */
    external fun cities(dataDir: String): String

    /** Selects the saved city with that key; false if there's none. */
    external fun selectCity(dataDir: String, key: String): Boolean

    /** The device's own position as the selected "📍" place (the watch). */
    external fun setDevice(dataDir: String, name: String, latitude: Double, longitude: Double): Boolean
}

data class WidgetHour(val label: String, val iconKind: String, val temperature: String)

/** A day in the large widget's table; `rain` is the amount or the chance, empty when dry. */
data class WidgetDay(val label: String, val iconKind: String, val rain: String, val min: String, val max: String)

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
    /** "Now", "+30 min", … under every other bar. */
    val nowcastLabels: List<String> = emptyList(),
    /** Rain falling or due within the hour (the main widget then shows it). */
    val rainSoon: Boolean = false,
    /** A practical tip ("Take an umbrella from 16h"); empty when none. */
    val advice: String = "",
    /** The strongest official warning and its level (1 yellow, 2 orange, 3 red; 0 none). */
    val warning: String = "",
    val warningLevel: Int = 0,
    /** Today and the next days. */
    val days: List<WidgetDay> = emptyList(),
    /** Background opacity, 0–1, and whether to use the wallpaper's colours. */
    val opacity: Double = 1.0,
    val wallpaper: Boolean = false,
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
            nowcastLabels = it.optJSONArray("nowcast_labels")?.let { l -> (0 until l.length()).map { i -> l.getString(i) } }
                ?: emptyList(),
            rainSoon = it.optBoolean("rain_soon"),
            advice = it.optString("advice"),
            warning = it.optString("warning"),
            warningLevel = it.optInt("warning_level"),
            days = it.optJSONArray("days")?.let { d ->
                (0 until d.length()).map { i ->
                    val day = d.getJSONObject(i)
                    WidgetDay(
                        day.getString("label"), day.getString("icon_kind"), day.getString("rain"),
                        day.getString("min"), day.getString("max"),
                    )
                }
            } ?: emptyList(),
            opacity = it.optDouble("opacity", 1.0),
            wallpaper = it.optBoolean("wallpaper"),
        )
    }
    return WidgetState(o.getBoolean("needs_refresh"), view)
}
