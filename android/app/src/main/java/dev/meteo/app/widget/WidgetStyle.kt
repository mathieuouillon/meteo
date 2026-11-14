package dev.meteo.app.widget

import android.content.Context
import android.content.res.Configuration
import android.os.Build

/**
 * The widget settings applied to its colours: the wallpaper's (Material You,
 * Android 12+) instead of the weather's, and a see-through background.
 */
fun WidgetView.styled(context: Context): WidgetView {
    var v = this
    if (wallpaper && Build.VERSION.SDK_INT >= 31) {
        val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK ==
            Configuration.UI_MODE_NIGHT_YES
        val c = { id: Int -> context.getColor(id) }
        v = if (night) {
            copy(
                dark = true,
                top = c(android.R.color.system_accent1_800), bottom = c(android.R.color.system_accent1_900),
                card = c(android.R.color.system_neutral1_800), ink = c(android.R.color.system_neutral1_50),
                inkSoft = c(android.R.color.system_neutral2_200), accent = c(android.R.color.system_accent1_200),
            )
        } else {
            copy(
                dark = false,
                top = c(android.R.color.system_accent1_100), bottom = c(android.R.color.system_accent1_200),
                card = c(android.R.color.system_neutral1_50), ink = c(android.R.color.system_neutral1_900),
                inkSoft = c(android.R.color.system_neutral2_700), accent = c(android.R.color.system_accent1_600),
            )
        }
    }
    if (opacity >= 1.0) return v
    val alpha = (opacity.coerceIn(0.0, 1.0) * 255).toInt() shl 24
    val see = { color: Int -> (color and 0x00FFFFFF) or alpha }
    return v.copy(top = see(v.top), bottom = see(v.bottom), card = see(v.card))
}
