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

private val white = ColorProvider(Color.White)
private val faded = ColorProvider(Color(0xCCFFFFFF))

private fun style(size: TextUnit, weight: FontWeight = FontWeight.Normal, color: ColorProvider = white) =
    TextStyle(color = color, fontSize = size, fontWeight = weight)

@Composable
private fun Content(view: WidgetView?) {
    val context = LocalContext.current
    val wide = LocalSize.current.width >= 250.dp
    val openApp = actionStartActivity(
        Intent().setComponent(ComponentName(context.packageName, "android.app.NativeActivity")),
    )
    val top = view?.skyTop ?: 0xFF2F80ED.toInt()
    val bottom = view?.skyBottom ?: 0xFF8EC5FC.toInt()

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
            Text("Météo\n$loading", style = style(15.sp))
            return@Box
        }
        Row(modifier = GlanceModifier.fillMaxSize(), verticalAlignment = Alignment.Vertical.CenterVertically) {
            Column {
                Text((if (view.isDevice) "📍 " else "") + view.city, style = style(15.sp, FontWeight.Bold), maxLines = 1)
                Row(verticalAlignment = Alignment.Vertical.CenterVertically) {
                    Text(view.icon, style = style(26.sp))
                    Spacer(GlanceModifier.width(4.dp))
                    Text(view.temperature, style = style(38.sp))
                }
                Text(view.condition, style = style(13.sp), maxLines = 1)
                Text(view.highLow, style = style(12.sp, color = faded), maxLines = 1)
            }
            if (wide) {
                Spacer(GlanceModifier.width(12.dp))
                Row(modifier = GlanceModifier.defaultWeight(), horizontalAlignment = Alignment.Horizontal.End) {
                    view.hours.forEach { h ->
                        Column(
                            modifier = GlanceModifier.width(38.dp),
                            horizontalAlignment = Alignment.Horizontal.CenterHorizontally,
                        ) {
                            Text(h.label, style = style(11.sp, color = faded))
                            Spacer(GlanceModifier.height(2.dp))
                            Text(h.icon, style = style(18.sp))
                            Spacer(GlanceModifier.height(2.dp))
                            Text(h.temperature, style = style(13.sp, FontWeight.Medium))
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
