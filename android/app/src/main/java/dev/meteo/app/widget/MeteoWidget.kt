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
import android.graphics.Typeface
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.text.TextUtils
import android.util.TypedValue
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
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
import androidx.glance.layout.ColumnScope
import androidx.glance.layout.ContentScale
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
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

class MeteoWidgetReceiver : RustWidgetReceiver(MeteoWidget())
class RainWidgetReceiver : RustWidgetReceiver(RainWidget())
class RoundWidgetReceiver : RustWidgetReceiver(RoundWidget())

/** Shared by the widgets' receivers. */
abstract class RustWidgetReceiver(widget: GlanceAppWidget) : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = widget

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
 * A widget showing the cached forecast of the city selected in the app.
 * Everything it displays comes from Rust (RustBridge); subclasses only lay
 * it out in `Body`.
 */
abstract class RustWidget : GlanceAppWidget() {
    @Composable
    abstract fun Body(view: WidgetView?)

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
            Body(state.view?.styled(context))
        }
    }
}

/**
 * Current weather, in the app's editorial style. Small: the temperature and
 * the weather. Wide: also the next hours, or the next 2 hours of rain when
 * it's coming. Large: also the advice, a warning, and the days ahead.
 */
class MeteoWidget : RustWidget() {
    // Laid out for its exact size: the large one shows as many days as fit.
    override val sizeMode = SizeMode.Exact

    @Composable
    override fun Body(view: WidgetView?) {
        val size = LocalSize.current
        Page(view) {
            if (view == null) {
                Loading(view)
            } else when {
                size.height >= 220.dp && size.width >= 250.dp -> Large(view)
                size.width >= 250.dp -> Wide(view)
                else -> Small(view)
            }
        }
    }
}

/** The next 2 hours of rain: the summary and 15-minute stems. */
class RainWidget : RustWidget() {
    override val sizeMode = SizeMode.Exact

    @Composable
    override fun Body(view: WidgetView?) {
        Page(view) {
            if (view == null) {
                Loading(view)
                return@Page
            }
            Masthead(view, view.nowcastTitle)
            Spacer(GlanceModifier.height(6.dp))
            SerifText(view.nowcastSummary, 17f, view.ink, maxLines = 2)
            Spacer(GlanceModifier.defaultWeight())
            Stems(view, 34.dp)
        }
    }
}

/** A round widget: the temperature on a paper disc. */
class RoundWidget : RustWidget() {
    override val sizeMode = SizeMode.Exact

    @Composable
    override fun Body(view: WidgetView?) {
        val size = LocalSize.current
        val side = minOf(size.width, size.height)
        Box(modifier = GlanceModifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Box(
                modifier = GlanceModifier
                    .size(side)
                    .cornerRadius(side / 2)
                    .background(ImageProvider(skyBitmap(view?.top ?: PAPER, view?.bottom ?: PAPER)), ContentScale.FillBounds)
                    .clickable(openApp(LocalContext.current)),
                contentAlignment = Alignment.Center,
            ) {
                if (view != null) {
                    Column(horizontalAlignment = Alignment.Horizontal.CenterHorizontally) {
                        Image(
                            ImageProvider(weatherIcon(view.iconKind, view.dark)),
                            contentDescription = view.condition,
                            modifier = GlanceModifier.size(side * 0.26f),
                        )
                        SerifText(view.temperature, side.value * 0.26f, view.ink, weight = 300)
                    }
                }
            }
        }
    }
}

// Before the first download: a plain paper page.
private const val PAPER = 0xFFF5EFE4.toInt()
private const val INK = 0xFF1E1A15.toInt()
private const val INK_SOFT = 0xFF7A6F60.toInt()

/** The page: paper (a faint wash from the top), rounded, opening the app. */
@Composable
private fun Page(view: WidgetView?, content: @Composable ColumnScope.() -> Unit) {
    Box(
        modifier = GlanceModifier
            .fillMaxSize()
            .cornerRadius(18.dp)
            .background(ImageProvider(skyBitmap(view?.top ?: PAPER, view?.bottom ?: PAPER)), ContentScale.FillBounds)
            .clickable(openApp(LocalContext.current))
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        Column(modifier = GlanceModifier.fillMaxSize(), content = content)
    }
}

@Composable
private fun Loading(view: WidgetView?) {
    val fr = LocalContext.current.resources.configuration.locales[0].language == "fr"
    SerifText("Météo", 20f, view?.ink ?: INK)
    Text(if (fr) "Chargement…" else "Loading…", style = style(12.sp, view?.inkSoft ?: INK_SOFT))
}

/** The city in small capitals over a rule, with an optional note on the right. */
@Composable
private fun Masthead(view: WidgetView, note: String = "") {
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        Row(modifier = GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.Vertical.CenterVertically) {
            Text(
                (if (view.isDevice) "📍 " else "") + view.city.uppercase(),
                style = style(10.sp, view.ink, FontWeight.Bold),
                maxLines = 1,
                modifier = GlanceModifier.defaultWeight(),
            )
            if (note.isNotEmpty()) Text(note.uppercase(), style = style(9.sp, view.inkSoft, FontWeight.Medium), maxLines = 1)
        }
        Spacer(GlanceModifier.height(4.dp))
        Box(modifier = GlanceModifier.fillMaxWidth().height(1.dp).background(ColorProvider(Color(view.ink)))) {}
    }
}

/** A thin rule in the quieter ink. */
@Composable
private fun Rule(view: WidgetView) {
    Box(modifier = GlanceModifier.fillMaxWidth().height(1.dp).background(ColorProvider(Color(view.inkSoft).copy(alpha = 0.35f)))) {}
}

/** The temperature large, the weather icon beside it. */
@Composable
private fun Temperature(view: WidgetView, size: Float) {
    Row(modifier = GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.Vertical.CenterVertically) {
        SerifText(view.temperature, size, view.ink, weight = 300)
        Spacer(GlanceModifier.defaultWeight())
        Image(
            ImageProvider(weatherIcon(view.iconKind, view.dark)),
            contentDescription = view.condition,
            modifier = GlanceModifier.size((size * 0.62f).dp),
        )
    }
}

/** Under the weather: the rain to come in the accent, or the day's high and low. */
@Composable
private fun Footnote(view: WidgetView) {
    if (view.rainSoon && view.nowcastSummary.isNotEmpty()) {
        Text(view.nowcastSummary, style = style(11.sp, view.accent, FontWeight.Medium), maxLines = 2)
    } else {
        Text(view.highLow, style = style(11.sp, view.inkSoft), maxLines = 1)
    }
}

@Composable
private fun ColumnScope.Small(view: WidgetView) {
    Masthead(view)
    Spacer(GlanceModifier.height(2.dp))
    Temperature(view, 40f)
    SerifText(view.condition, 15f, view.ink, italic = true)
    Spacer(GlanceModifier.defaultWeight())
    Footnote(view)
}

@Composable
private fun ColumnScope.Wide(view: WidgetView) {
    Masthead(view)
    Spacer(GlanceModifier.height(4.dp))
    Row(modifier = GlanceModifier.fillMaxSize()) {
        Column(modifier = GlanceModifier.width(112.dp).fillMaxHeight()) {
            SerifText(view.temperature, 40f, view.ink, weight = 300)
            SerifText(view.condition, 14f, view.ink, italic = true, maxWidth = 112.dp)
            Spacer(GlanceModifier.defaultWeight())
            Footnote(view)
        }
        Spacer(GlanceModifier.width(10.dp))
        Column(modifier = GlanceModifier.defaultWeight().fillMaxHeight(), verticalAlignment = Alignment.Vertical.Bottom) {
            if (view.rainSoon) Stems(view, 36.dp) else Hours(view)
        }
    }
}

@Composable
private fun ColumnScope.Large(view: WidgetView) {
    Masthead(view, view.updated)
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        if (view.warning.isNotEmpty()) {
            Spacer(GlanceModifier.height(6.dp))
            Warning(view)
        }
        Spacer(GlanceModifier.height(2.dp))
        Temperature(view, 48f)
        SerifText(view.condition, 17f, view.ink, italic = true)
        if (view.advice.isNotEmpty()) SerifText(view.advice, 14f, view.accent, italic = true, maxLines = 2)
        Text(view.highLow + " · " + view.feelsLike, style = style(11.sp, view.inkSoft), maxLines = 1)
    }
    Spacer(GlanceModifier.defaultWeight())
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        Rule(view)
        Spacer(GlanceModifier.height(6.dp))
        if (view.rainSoon) Stems(view, 30.dp) else Hours(view)
        Spacer(GlanceModifier.height(6.dp))
        Rule(view)
    }
    // As many days as fit under the rest (about 21 dp a row).
    val above = 236 - (if (view.warning.isEmpty()) 20 else 0) - (if (view.advice.isEmpty()) 18 else 0)
    val rows = ((LocalSize.current.height.value - above) / 21f).toInt().coerceIn(0, view.days.size)
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        view.days.take(rows).forEach { DayLine(view, it) }
    }
}

/** The strongest warning: a coloured mark and its title. */
@Composable
private fun Warning(view: WidgetView) {
    val colour = when (view.warningLevel) {
        3 -> 0xFFD93A2B.toInt()
        2 -> 0xFFF08A24.toInt()
        else -> 0xFFE8B931.toInt()
    }
    Row(verticalAlignment = Alignment.Vertical.CenterVertically) {
        Box(modifier = GlanceModifier.size(8.dp).cornerRadius(1.dp).background(ColorProvider(Color(colour)))) {}
        Spacer(GlanceModifier.width(6.dp))
        Text(view.warning, style = style(11.sp, view.ink, FontWeight.Medium), maxLines = 1)
    }
}

/** The next hours: time, icon, temperature in the serif. */
@Composable
private fun Hours(view: WidgetView) {
    Row(modifier = GlanceModifier.fillMaxWidth()) {
        view.hours.forEach { h ->
            Column(modifier = GlanceModifier.defaultWeight(), horizontalAlignment = Alignment.Horizontal.CenterHorizontally) {
                Text(h.label, style = style(10.sp, view.inkSoft), maxLines = 1)
                Spacer(GlanceModifier.height(3.dp))
                Image(ImageProvider(weatherIcon(h.iconKind, view.dark)), contentDescription = null, modifier = GlanceModifier.size(20.dp))
                Spacer(GlanceModifier.height(2.dp))
                SerifText(h.temperature, 15f, view.ink)
            }
        }
    }
}

/** The next 2 hours: a stem every 15 minutes on a baseline, times under it. */
@Composable
private fun Stems(view: WidgetView, height: androidx.compose.ui.unit.Dp) {
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        Row(modifier = GlanceModifier.fillMaxWidth().height(height), verticalAlignment = Alignment.Vertical.Bottom) {
            view.nowcast.forEach { (h, wet) ->
                Box(modifier = GlanceModifier.defaultWeight().fillMaxHeight(), contentAlignment = Alignment.BottomStart) {
                    if (wet) {
                        Box(
                            modifier = GlanceModifier
                                .width(4.dp)
                                .height(3.dp + (height - 4.dp) * h.toFloat())
                                .background(ColorProvider(Color(view.accent))),
                        ) {}
                    }
                }
            }
        }
        Box(modifier = GlanceModifier.fillMaxWidth().height(1.dp).background(ColorProvider(Color(view.ink)))) {}
        Spacer(GlanceModifier.height(2.dp))
        Row(modifier = GlanceModifier.fillMaxWidth()) {
            for (i in view.nowcastLabels.indices step 2) {
                Box(modifier = GlanceModifier.defaultWeight()) {
                    Text(view.nowcastLabels[i], style = style(9.sp, view.inkSoft), maxLines = 1)
                }
            }
        }
    }
}

/** One day of the table: name, icon, rain in the accent, low and high. */
@Composable
private fun DayLine(view: WidgetView, d: WidgetDay) {
    Row(modifier = GlanceModifier.fillMaxWidth().padding(vertical = 2.dp), verticalAlignment = Alignment.Vertical.CenterVertically) {
        Box(modifier = GlanceModifier.width(78.dp)) { SerifText(d.label, 15f, view.ink, maxWidth = 76.dp) }
        Image(ImageProvider(weatherIcon(d.iconKind, view.dark)), contentDescription = null, modifier = GlanceModifier.size(18.dp))
        Spacer(GlanceModifier.width(8.dp))
        Text(d.rain, style = style(11.sp, view.accent, FontWeight.Medium), maxLines = 1, modifier = GlanceModifier.defaultWeight())
        Text(d.min, style = style(12.sp, view.inkSoft), maxLines = 1)
        Spacer(GlanceModifier.width(8.dp))
        Box(modifier = GlanceModifier.width(30.dp), contentAlignment = Alignment.CenterEnd) { SerifText(d.max, 15f, view.ink) }
    }
}

/** The app's serif (Newsreader, from the APK's assets), by style and weight. */
private object Serif {
    private val faces = HashMap<String, Typeface>()

    fun face(context: Context, italic: Boolean, weight: Int, opticalSize: Float): Typeface {
        val file = if (italic) "Newsreader-Italic.ttf" else "Newsreader.ttf"
        val opsz = opticalSize.coerceIn(6f, 72f).toInt()
        return faces.getOrPut("$file/$weight/$opsz") {
            runCatching {
                Typeface.Builder(context.assets, file).setFontVariationSettings("'wght' $weight, 'opsz' $opsz").build()
            }.getOrNull() ?: Typeface.create(Typeface.SERIF, if (italic) Typeface.ITALIC else Typeface.NORMAL)
        }
    }
}

/**
 * Text in the app's serif. Glance can't use a font file, so the text is
 * drawn into an image (wrapped to `maxLines` within `maxWidth`, then
 * ellipsized) and read out by its content description.
 */
@Composable
private fun SerifText(
    text: String,
    sizeSp: Float,
    color: Int,
    italic: Boolean = false,
    weight: Int = 400,
    maxLines: Int = 1,
    maxWidth: androidx.compose.ui.unit.Dp? = null,
) {
    if (text.isEmpty()) return
    val context = LocalContext.current
    val metrics = context.resources.displayMetrics
    val paint = TextPaint(Paint.ANTI_ALIAS_FLAG).apply {
        typeface = Serif.face(context, italic, weight, sizeSp)
        textSize = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, sizeSp, metrics)
        this.color = color
    }
    val room = (maxWidth ?: (LocalSize.current.width - 28.dp)).value * metrics.density
    val width = minOf(StaticLayout.getDesiredWidth(text, paint), room).toInt().coerceAtLeast(1) + 2
    val layout = StaticLayout.Builder.obtain(text, 0, text.length, paint, width)
        .setAlignment(Layout.Alignment.ALIGN_NORMAL)
        .setIncludePad(false)
        .setMaxLines(maxLines)
        .setEllipsize(TextUtils.TruncateAt.END)
        .build()
    val bitmap = Bitmap.createBitmap(width, layout.height.coerceAtLeast(1), Bitmap.Config.ARGB_8888)
    layout.draw(Canvas(bitmap))
    Image(
        ImageProvider(bitmap),
        contentDescription = text,
        modifier = GlanceModifier.width((bitmap.width / metrics.density).dp).height((bitmap.height / metrics.density).dp),
    )
}

private fun openApp(context: Context) = actionStartActivity(
    Intent().setComponent(ComponentName(context.packageName, "android.app.NativeActivity")),
)

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

/** A small vertical gradient bitmap, since Glance has no gradient backgrounds. */
private fun skyBitmap(top: Int, bottom: Int): Bitmap {
    val bitmap = Bitmap.createBitmap(8, 64, Bitmap.Config.ARGB_8888)
    val paint = Paint().apply { shader = LinearGradient(0f, 0f, 0f, 64f, top, bottom, Shader.TileMode.CLAMP) }
    Canvas(bitmap).drawRect(0f, 0f, 8f, 64f, paint)
    return bitmap
}
