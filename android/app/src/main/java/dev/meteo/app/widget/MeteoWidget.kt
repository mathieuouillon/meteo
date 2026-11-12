package dev.meteo.app.widget

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.longPreferencesKey
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import dev.meteo.app.R
import androidx.glance.layout.fillMaxHeight
import androidx.glance.layout.size
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.glance.currentState
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ContentScale
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Bumped on every update so a widget session that's already running reloads the cache. */
private val REVISION = longPreferencesKey("revision")

class MeteoWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = MeteoWidget()

    // Sent by the 30-minute timer and by the app after each download (Rust,
    // location_android::notify_widgets). Glance's default handling only
    // re-reads its state store in a running session, so change that store.
    override fun onUpdate(context: Context, appWidgetManager: AppWidgetManager, appWidgetIds: IntArray) {
        val pending = goAsync()
        CoroutineScope(Dispatchers.Default).launch {
            try {
                val glanceIds = GlanceAppWidgetManager(context)
                for (appWidgetId in appWidgetIds) {
                    val id = glanceIds.getGlanceIdBy(appWidgetId)
                    updateAppWidgetState(context, id) { it[REVISION] = System.currentTimeMillis() }
                    glanceAppWidget.update(context, id)
                }
            } finally {
                pending.finish()
            }
        }
    }
}

/**
 * Shows the cached forecast of the city selected in the app. Everything it
 * displays comes from Rust (RustBridge); this file only lays it out.
 */
class MeteoWidget : GlanceAppWidget() {
    companion object {
        private val SMALL = DpSize(110.dp, 110.dp)
        private val WIDE = DpSize(250.dp, 110.dp)
    }

    override val sizeMode = SizeMode.Responsive(setOf(SMALL, WIDE))

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val dir = context.filesDir.absolutePath
        val language = context.resources.configuration.locales[0].language
        provideContent {
            val revision = currentState<Preferences>()[REVISION] ?: 0L
            // Reload from the cache whenever the receiver bumps the revision.
            var state by remember(revision) { mutableStateOf(parseWidgetState(RustBridge.state(dir, language))) }
            // Show the cache at once, then download if it's old or missing.
            LaunchedEffect(revision) {
                if (state.needsRefresh) {
                    val ok = withContext(Dispatchers.IO) { RustBridge.refresh(dir) }
                    if (ok) state = withContext(Dispatchers.IO) { parseWidgetState(RustBridge.state(dir, language)) }
                }
            }
            Content(state.view)
        }
    }
}

private fun style(size: TextUnit, color: Int, weight: FontWeight = FontWeight.Normal) =
    TextStyle(color = ColorProvider(Color(color)), fontSize = size, fontWeight = weight)

/** The drawn weather icon (res/drawable/wx_*, from scripts/gen-widget-icons.py). */
private fun weatherIcon(kind: String, dark: Boolean): Int = when (kind) {
    "clear-day" -> if (dark) R.drawable.wx_clear_day_dark else R.drawable.wx_clear_day_light
    "clear-night" -> if (dark) R.drawable.wx_clear_night_dark else R.drawable.wx_clear_night_light
    "partly-day" -> if (dark) R.drawable.wx_partly_day_dark else R.drawable.wx_partly_day_light
    "partly-night" -> if (dark) R.drawable.wx_partly_night_dark else R.drawable.wx_partly_night_light
    "fog" -> if (dark) R.drawable.wx_fog_dark else R.drawable.wx_fog_light
    "drizzle" -> if (dark) R.drawable.wx_drizzle_dark else R.drawable.wx_drizzle_light
    "rain" -> if (dark) R.drawable.wx_rain_dark else R.drawable.wx_rain_light
    "snow" -> if (dark) R.drawable.wx_snow_dark else R.drawable.wx_snow_light
    "storm" -> if (dark) R.drawable.wx_storm_dark else R.drawable.wx_storm_light
    else -> if (dark) R.drawable.wx_cloudy_dark else R.drawable.wx_cloudy_light
}

@Composable
private fun Content(view: WidgetView?) {
    val context = LocalContext.current
    val wide = LocalSize.current.width >= 250.dp
    val openApp = actionStartActivity(
        Intent().setComponent(ComponentName(context.packageName, "android.app.NativeActivity")),
    )
    // Before the first download: the clear-day palette.
    val top = view?.top ?: 0xFFF8E9D2.toInt()
    val bottom = view?.bottom ?: 0xFFF2D3A6.toInt()
    val ink = view?.ink ?: 0xFF3B2A12.toInt()
    val inkSoft = view?.inkSoft ?: 0xFF7D6446.toInt()

    Box(
        modifier = GlanceModifier
            .fillMaxSize()
            .cornerRadius(24.dp)
            .background(ImageProvider(skyBitmap(top, bottom)), ContentScale.FillBounds)
            .clickable(openApp)
            .padding(14.dp),
    ) {
        if (view == null) {
            val loading = if (context.resources.configuration.locales[0].language == "fr") "Chargement…" else "Loading…"
            Column {
                Text("Météo", style = style(15.sp, ink, FontWeight.Bold))
                Text(loading, style = style(12.sp, inkSoft))
            }
            return@Box
        }
        val icon = { kind: String -> ImageProvider(weatherIcon(kind, view.dark)) }
        Row(modifier = GlanceModifier.fillMaxSize(), verticalAlignment = Alignment.Vertical.CenterVertically) {
            // Now: city, icon and big temperature, condition, day/night.
            Column(modifier = GlanceModifier.fillMaxHeight()) {
                Text(
                    (if (view.isDevice) "📍 " else "") + view.city,
                    style = style(13.sp, ink, FontWeight.Bold), maxLines = 1,
                )
                Spacer(GlanceModifier.defaultWeight())
                Row(verticalAlignment = Alignment.Vertical.CenterVertically) {
                    Image(icon(view.iconKind), contentDescription = view.condition, modifier = GlanceModifier.size(36.dp))
                    Spacer(GlanceModifier.width(6.dp))
                    Text(view.temperature, style = style(38.sp, ink, FontWeight.Bold))
                }
                Text(view.condition, style = style(13.sp, ink, FontWeight.Medium), maxLines = 1)
                Text(view.highLow, style = style(11.sp, inkSoft), maxLines = 1)
            }
            // Wide: the next hours, on a card in the palette.
            if (wide) {
                Spacer(GlanceModifier.width(12.dp))
                Row(
                    modifier = GlanceModifier
                        .defaultWeight()
                        .cornerRadius(18.dp)
                        .background(ColorProvider(Color(view.card).copy(alpha = 0.7f)))
                        .padding(vertical = 10.dp, horizontal = 4.dp),
                    horizontalAlignment = Alignment.Horizontal.CenterHorizontally,
                ) {
                    view.hours.forEach { h ->
                        Column(
                            modifier = GlanceModifier.defaultWeight(),
                            horizontalAlignment = Alignment.Horizontal.CenterHorizontally,
                        ) {
                            Text(h.label, style = style(11.sp, inkSoft), maxLines = 1)
                            Spacer(GlanceModifier.height(4.dp))
                            Image(icon(h.iconKind), contentDescription = null, modifier = GlanceModifier.size(22.dp))
                            Spacer(GlanceModifier.height(4.dp))
                            Text(h.temperature, style = style(13.sp, ink, FontWeight.Bold), maxLines = 1)
                        }
                    }
                }
            }
        }
    }
}

/** A small vertical gradient bitmap, since Glance has no gradient backgrounds. */
private fun skyBitmap(top: Int, bottom: Int): Bitmap {
    val bitmap = Bitmap.createBitmap(8, 64, Bitmap.Config.ARGB_8888)
    val paint = Paint().apply { shader = LinearGradient(0f, 0f, 0f, 64f, top, bottom, Shader.TileMode.CLAMP) }
    Canvas(bitmap).drawRect(0f, 0f, 8f, 64f, paint)
    return bitmap
}
