package dev.meteo.app.wear

import androidx.concurrent.futures.CallbackToFutureAdapter
import androidx.wear.protolayout.ActionBuilders
import androidx.wear.protolayout.ColorBuilders.argb
import androidx.wear.protolayout.DimensionBuilders.dp
import androidx.wear.protolayout.DimensionBuilders.sp
import androidx.wear.protolayout.LayoutElementBuilders
import androidx.wear.protolayout.LayoutElementBuilders.LayoutElement
import androidx.wear.protolayout.ModifiersBuilders
import androidx.wear.protolayout.ResourceBuilders
import androidx.wear.protolayout.TimelineBuilders
import androidx.wear.tiles.RequestBuilders
import androidx.wear.tiles.TileBuilders
import androidx.wear.tiles.TileService
import com.google.common.util.concurrent.ListenableFuture
import dev.meteo.app.R
import dev.meteo.app.widget.RustBridge
import dev.meteo.app.widget.parseWidgetState

/**
 * The Wear OS tile: city, weather icon, temperature, condition, day and
 * night. Refreshed every 30 minutes; tapping it opens the watch app.
 */
class WeatherTileService : TileService() {
    override fun onTileRequest(requestParams: RequestBuilders.TileRequest): ListenableFuture<TileBuilders.Tile> {
        val dir = filesDir.absolutePath
        val language = resources.configuration.locales[0].language
        val state = parseWidgetState(RustBridge.state(dir, language))
        if (state.needsRefresh) WatchRefresh.start(this)
        val view = state.view
        val content: LayoutElement = if (view == null) {
            text(if (language == "fr") "Chargement…" else "Loading…", 16f, GREY)
        } else {
            column(
                text(view.city, 14f, GREY),
                spacer(4f),
                row(image(view.iconKind, 36f), spacer(6f, horizontal = true), text(view.temperature, 40f, WHITE, bold = true)),
                text(view.condition, 15f, WHITE),
                spacer(2f),
                text(view.highLow, 13f, GREY),
            )
        }
        val root = LayoutElementBuilders.Box.Builder()
            .setWidth(androidx.wear.protolayout.DimensionBuilders.expand())
            .setHeight(androidx.wear.protolayout.DimensionBuilders.expand())
            .setModifiers(openApp())
            .addContent(content)
            .build()
        val tile = TileBuilders.Tile.Builder()
            .setResourcesVersion(RESOURCES_VERSION)
            .setFreshnessIntervalMillis(30 * 60 * 1000L)
            .setTileTimeline(TimelineBuilders.Timeline.fromLayoutElement(root))
            .build()
        return CallbackToFutureAdapter.getFuture { it.set(tile); "tile" }
    }

    override fun onTileResourcesRequest(
        requestParams: RequestBuilders.ResourcesRequest,
    ): ListenableFuture<ResourceBuilders.Resources> {
        val resources = ResourceBuilders.Resources.Builder().setVersion(RESOURCES_VERSION)
        for ((kind, res) in ICONS) {
            resources.addIdToImageMapping(
                kind,
                ResourceBuilders.ImageResource.Builder()
                    .setAndroidResourceByResId(
                        ResourceBuilders.AndroidImageResourceByResId.Builder().setResourceId(res).build(),
                    )
                    .build(),
            )
        }
        val built = resources.build()
        return CallbackToFutureAdapter.getFuture { it.set(built); "resources" }
    }

    private fun openApp() = ModifiersBuilders.Modifiers.Builder()
        .setClickable(
            ModifiersBuilders.Clickable.Builder()
                .setId("open")
                .setOnClick(
                    ActionBuilders.LaunchAction.Builder()
                        .setAndroidActivity(
                            ActionBuilders.AndroidActivity.Builder()
                                .setPackageName(packageName)
                                .setClassName(WearActivity::class.java.name)
                                .build(),
                        )
                        .build(),
                )
                .build(),
        )
        .build()

    private fun text(s: String, size: Float, color: Int, bold: Boolean = false): LayoutElement =
        LayoutElementBuilders.Text.Builder()
            .setText(s)
            .setMaxLines(1)
            .setFontStyle(
                LayoutElementBuilders.FontStyle.Builder()
                    .setSize(sp(size))
                    .setColor(argb(color))
                    .setWeight(if (bold) LayoutElementBuilders.FONT_WEIGHT_BOLD else LayoutElementBuilders.FONT_WEIGHT_NORMAL)
                    .build(),
            )
            .build()

    private fun image(kind: String, size: Float): LayoutElement =
        LayoutElementBuilders.Image.Builder()
            .setResourceId(if (ICONS.containsKey(kind)) kind else "cloudy")
            .setWidth(dp(size))
            .setHeight(dp(size))
            .build()

    private fun spacer(size: Float, horizontal: Boolean = false): LayoutElement =
        LayoutElementBuilders.Spacer.Builder().apply {
            if (horizontal) setWidth(dp(size)) else setHeight(dp(size))
        }.build()

    private fun column(vararg items: LayoutElement): LayoutElement =
        LayoutElementBuilders.Column.Builder()
            .setHorizontalAlignment(LayoutElementBuilders.HORIZONTAL_ALIGN_CENTER)
            .apply { items.forEach { addContent(it) } }
            .build()

    private fun row(vararg items: LayoutElement): LayoutElement =
        LayoutElementBuilders.Row.Builder()
            .setVerticalAlignment(LayoutElementBuilders.VERTICAL_ALIGN_CENTER)
            .apply { items.forEach { addContent(it) } }
            .build()

    companion object {
        private const val RESOURCES_VERSION = "1"
        private const val WHITE = 0xFFFFFFFF.toInt()
        private const val GREY = 0xFFB8BEC8.toInt()

        /** The drawn weather icons (light lines for the watch's dark face). */
        private val ICONS = mapOf(
            "clear-day" to R.drawable.wx_clear_day_dark,
            "clear-night" to R.drawable.wx_clear_night_dark,
            "partly-day" to R.drawable.wx_partly_day_dark,
            "partly-night" to R.drawable.wx_partly_night_dark,
            "cloudy" to R.drawable.wx_cloudy_dark,
            "fog" to R.drawable.wx_fog_dark,
            "drizzle" to R.drawable.wx_drizzle_dark,
            "rain" to R.drawable.wx_rain_dark,
            "snow" to R.drawable.wx_snow_dark,
            "storm" to R.drawable.wx_storm_dark,
        )
    }
}
