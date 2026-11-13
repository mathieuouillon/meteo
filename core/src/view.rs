//! Ready-to-display data for the main screen.
//!
//! UIs bind these fields directly: no formatting or layout maths in Slint/JS.

use serde::Serialize;

use crate::chart::{ChartGeometry, temperature_chart};
use crate::date::weekday_index;
use crate::i18n::Lang;
use crate::model::{Day, Forecast};
use crate::landscape::{Landscape, Phase, landscape, seed_for};
use crate::palette::{Palette, icon_kind};
use crate::scene::SceneSpec;

/// Horizontal space per hour in a day's detail chart, in logical pixels.
pub const HOUR_STEP: f64 = 56.0;
/// Height of the temperature curve in a day's detail chart.
pub const CHART_HEIGHT: f64 = 110.0;
/// Hours the widgets can show from now.
pub const HOURS_SHOWN: usize = 24;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MainScreen {
    pub city: String,
    pub temperature: String,
    pub condition: String,
    /// Emoji (for the widget) and the drawn icon's name (for the app).
    pub icon: String,
    pub icon_kind: String,
    pub high_low: String,
    /// "Feels like 9°".
    pub feels_like: String,
    pub sky_top: u32,
    pub sky_bottom: u32,
    pub sky_top_css: String,
    pub sky_bottom_css: String,
    /// The next hours from now (used by the widgets).
    pub hours: Vec<HourView>,
    pub days: Vec<DayView>,
    pub details: DetailsView,
    /// What the animated background shows.
    pub scene: SceneSpec,
    /// The nature scene behind everything.
    pub landscape: Landscape,
    /// The UI's colours, from the weather and time of day.
    pub palette: Palette,
}

/// One of the next hours, for the widgets.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourView {
    pub label: String,
    pub temperature: String,
    pub icon: String,
    pub icon_kind: String,
}

/// The details tiles: each value with a short explanation, plus what the
/// compass and the sun's arc need.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetailsView {
    pub feels_like: String,
    pub feels_note: String,
    pub humidity: String,
    /// 0–1, for the humidity gauge.
    pub humidity_fraction: f64,
    pub dew_point: String,
    /// "20", with the unit apart so the number can be big.
    pub wind_speed: String,
    pub wind_unit: String,
    /// "Gusts 45 km/h"; empty if unknown.
    pub wind_gusts: String,
    /// "From the SW".
    pub wind_from: String,
    /// Where the wind blows *to*, in degrees clockwise from north.
    pub wind_arrow: f64,
    /// That arrow, drawn in a 64×64 compass box (centre 32,32).
    pub wind_arrow_path: String,
    /// "07:42"; empty if unknown.
    pub sunrise: String,
    pub sunset: String,
    pub daylight: String,
    /// The sun's position along its arc, 0 (sunrise) – 1 (sunset), clamped.
    pub sun_progress: f64,
    pub sun_up: bool,
    /// The sun's dot on its arc, in the 100×56 box the arc is drawn in
    /// (`SUN_ARC_PATH`).
    pub sun_x: f64,
    pub sun_y: f64,
    /// "3"; "--" if unknown.
    pub uv: String,
    pub uv_level: String,
    /// Where the UV marker sits on its scale, 0–1 (0 to 11+).
    pub uv_position: f64,
    /// False when the source has no UV index.
    pub uv_known: bool,
    /// "1013 hPa"; "--" if unknown.
    pub pressure: String,
    pub pressure_note: String,
}

impl MainScreen {
    pub fn from_forecast(f: &Forecast, lang: Lang) -> Self {
        let c = &f.current;
        let sky = c.condition.sky(c.is_day);
        let hours = f
            .hours
            .iter()
            .take(HOURS_SHOWN)
            .enumerate()
            .map(|(i, h)| HourView {
                label: if i == 0 { lang.now().into() } else { h.label.clone() },
                temperature: temp(h.temperature),
                icon: h.condition.icon(h.is_day).into(),
                icon_kind: icon_kind(h.condition, h.is_day).into(),
            })
            .collect();
        // The current hour, "15" in "…T15:00", marks "Now" in today's chart.
        let now = f.hours.first().map(|h| h.time.clone());
        let today = f.days.first();
        let (rise, set) = (today.and_then(|d| d.sunrise.as_deref()), today.and_then(|d| d.sunset.as_deref()));
        let phase = Phase::at(&c.time, rise, set, c.is_day);
        let progress = sun_progress(&c.time, rise, set);
        let palette = Palette::for_weather(c.condition, phase);
        let mut land = landscape(c.condition, phase, progress, seed_for(&f.location));
        // Quiet, tone-on-tone hills in the palette's colours.
        for (layer, color) in land.layers.iter_mut().zip(palette.hills()) {
            layer.color = color;
        }
        land.ground = palette.hills()[3];

        Self {
            city: f.location.clone(),
            temperature: temp(c.temperature),
            condition: lang.condition(c.condition).into(),
            icon: c.condition.icon(c.is_day).into(),
            icon_kind: icon_kind(c.condition, c.is_day).into(),
            high_low: lang.day_night(&temp(f.today_max), &temp(f.today_min)),
            feels_like: lang.feels_like(&temp(c.apparent_temperature)),
            sky_top: sky.top,
            sky_bottom: sky.bottom,
            sky_top_css: sky.top_css(),
            sky_bottom_css: sky.bottom_css(),
            hours,
            days: DayView::list(&f.days, now.as_deref(), lang),
            details: DetailsView::new(f, rise, set, progress, lang),
            scene: SceneSpec::for_weather(c.condition, c.is_day, c.wind_speed, c.wind_direction),
            landscape: land,
            palette,
        }
    }
}

impl DetailsView {
    fn new(f: &Forecast, rise: Option<&str>, set: Option<&str>, progress: f64, lang: Lang) -> Self {
        let c = &f.current;
        let clock = |t: Option<&str>| t.and_then(|t| t.get(11..16)).unwrap_or("").to_string();
        let minutes = |t: &str| -> Option<i64> { Some(t.get(11..13)?.parse::<i64>().ok()? * 60 + t.get(14..16)?.parse::<i64>().ok()?) };
        let daylight = match (rise.and_then(minutes), set.and_then(minutes)) {
            (Some(r), Some(s)) if s > r => lang.daylight(s - r),
            _ => String::new(),
        };
        Self {
            feels_like: temp(c.apparent_temperature),
            feels_note: lang.feels_note(c.temperature, c.apparent_temperature).into(),
            humidity: format!("{:.0}%", c.humidity),
            humidity_fraction: (c.humidity / 100.0).clamp(0.0, 1.0),
            dew_point: c.dew_point.map(|d| lang.dew_point(&temp(d))).unwrap_or_default(),
            wind_speed: format!("{:.0}", c.wind_speed),
            wind_unit: "km/h".into(),
            wind_gusts: c.wind_gusts.map(|g| lang.gusts(&format!("{g:.0} km/h"))).unwrap_or_default(),
            wind_from: lang.wind_from(c.wind_direction),
            wind_arrow: (c.wind_direction + 180.0).rem_euclid(360.0),
            wind_arrow_path: arrow_path((c.wind_direction + 180.0).rem_euclid(360.0)),
            sunrise: clock(rise),
            sunset: clock(set),
            daylight,
            sun_progress: progress.clamp(0.0, 1.0),
            sun_up: (0.0..=1.0).contains(&progress),
            sun_x: SUN_ARC_CENTER.0 - SUN_ARC_RADIUS * (std::f64::consts::PI * progress.clamp(0.0, 1.0)).cos(),
            sun_y: SUN_ARC_CENTER.1 - SUN_ARC_RADIUS * (std::f64::consts::PI * progress.clamp(0.0, 1.0)).sin(),
            uv: c.uv_index.map_or("--".into(), |u| format!("{:.0}", u.round())),
            uv_level: c.uv_index.map_or_else(|| lang.not_available().to_string(), |u| lang.uv_level(u).to_string()),
            uv_known: c.uv_index.is_some(),
            uv_position: c.uv_index.map_or(0.0, |u| (u / 11.0).clamp(0.0, 1.0)),
            pressure: c.pressure.map_or("--".into(), |p| format!("{p:.0} hPa")),
            pressure_note: c.pressure.map_or_else(|| lang.not_available().to_string(), |p| lang.pressure_level(p).to_string()),
        }
    }
}

/// The sun's half-circle arc, drawn in a 100×56 box.
pub const SUN_ARC_PATH: &str = "M 8 50 A 42 42 0 0 1 92 50";
const SUN_ARC_CENTER: (f64, f64) = (50.0, 50.0);
const SUN_ARC_RADIUS: f64 = 42.0;

/// A compass arrow pointing `degrees` clockwise from north, in a 64×64 box.
fn arrow_path(degrees: f64) -> String {
    let (sin, cos) = degrees.to_radians().sin_cos();
    // Pointing north (up): tip, right barb, notch, left barb.
    let points = [(0.0, -20.0), (8.0, 12.0), (0.0, 6.0), (-8.0, 12.0)];
    let mut path = String::new();
    for (i, (dx, dy)) in points.iter().enumerate() {
        // Clockwise rotation in screen coordinates (y down).
        let (x, y) = (32.0 + dx * cos - dy * sin, 32.0 + dx * sin + dy * cos);
        path += &format!("{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" });
    }
    path + "Z"
}

/// How far the sun is between sunrise (0) and sunset (1); 0.5 if unknown.
fn sun_progress(now: &str, sunrise: Option<&str>, sunset: Option<&str>) -> f64 {
    let minutes = |t: &str| -> Option<f64> {
        Some(t.get(11..13)?.parse::<f64>().ok()? * 60.0 + t.get(14..16)?.parse::<f64>().ok()?)
    };
    match (minutes(now), sunrise.and_then(minutes), sunset.and_then(minutes)) {
        (Some(n), Some(r), Some(s)) if s > r => (n - r) / (s - r),
        _ => 0.5,
    }
}

/// One row of the 7-day list, and the hour-by-hour chart shown when it's opened.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayView {
    pub label: String,
    pub icon: String,
    pub icon_kind: String,
    /// Empty when rain is unlikely.
    pub precipitation: String,
    pub min: String,
    pub max: String,
    /// Where this day's min–max bar starts and ends, as fractions (0–1) of
    /// the whole week's temperature range, so all rows share one scale.
    pub bar_start: f64,
    pub bar_end: f64,
    /// The day's total, e.g. "3.1 mm"; empty when dry.
    pub rain_total: String,
    /// The day's temperature curve (one point per hour).
    pub chart: ChartGeometry,
    pub hours: Vec<DayHourView>,
    /// Hour the chart opens at: now for today, 06h for other days.
    pub first_hour: usize,
    /// The tallest rain bar of the day (0–1), so the chart only reserves
    /// the room that day needs.
    pub rain_peak: f64,
    /// Whether any hour shows a chance of rain (some sources have none).
    pub has_chance: bool,
}

/// One hour of a day's chart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayHourView {
    /// "15h", or "Now" for the current hour.
    pub label: String,
    pub is_now: bool,
    pub temperature: String,
    pub icon: String,
    pub icon_kind: String,
    /// Chance of rain, e.g. "40%"; empty when unlikely.
    pub precipitation: String,
    /// Expected amount, e.g. "1.2"; empty when dry.
    pub rain: String,
    /// Rain bar height, 0–1, on a scale shared by the whole week.
    pub rain_height: f64,
    /// This hour's point on the temperature curve.
    pub x: f64,
    pub y: f64,
}

/// Rain bars are scaled to the wettest hour of the week, but never so that
/// a drizzle fills the chart.
const RAIN_SCALE_MIN_MM: f64 = 2.0;
/// Where other days' charts open.
const MORNING: usize = 6;

impl DayView {
    /// `now`: the current hour's time ("2026-11-12T15:00"), marked "Now".
    fn list(days: &[Day], now: Option<&str>, lang: Lang) -> Vec<Self> {
        let lo = days.iter().map(|d| d.min).fold(f64::INFINITY, f64::min);
        let hi = days.iter().map(|d| d.max).fold(f64::NEG_INFINITY, f64::max);
        let range = (hi - lo).max(1.0);
        let wettest = days
            .iter()
            .flat_map(|d| d.hours.iter().map(|h| h.precipitation))
            .fold(RAIN_SCALE_MIN_MM, f64::max);
        days.iter()
            .enumerate()
            .map(|(day_index, d)| {
                let chart = temperature_chart(&d.hours, HOUR_STEP, CHART_HEIGHT, 30.0, 10.0);
                let now_index = now.and_then(|t| d.hours.iter().position(|h| h.time == t));
                let hours: Vec<DayHourView> = d
                    .hours
                    .iter()
                    .zip(&chart.points)
                    .enumerate()
                    .map(|(i, (h, p))| DayHourView {
                        label: if Some(i) == now_index { lang.now().into() } else { h.label.clone() },
                        is_now: Some(i) == now_index,
                        temperature: temp(h.temperature),
                        icon: h.condition.icon(h.is_day).into(),
                        icon_kind: icon_kind(h.condition, h.is_day).into(),
                        precipitation: rain_chance(h.precipitation_probability),
                        rain: rain_amount(h.precipitation, false),
                        rain_height: (h.precipitation / wettest).clamp(0.0, 1.0),
                        x: p.x,
                        y: p.y,
                    })
                    .collect();
                Self {
                    first_hour: now_index.unwrap_or(MORNING.min(d.hours.len().saturating_sub(1))),
                    label: if day_index == 0 {
                        lang.today().into()
                    } else {
                        weekday_index(&d.date).map_or_else(|| d.label.clone(), |w| lang.weekday(w).into())
                    },
                    icon: d.condition.icon(true).into(),
                    icon_kind: icon_kind(d.condition, true).into(),
                    precipitation: rain_chance(d.precipitation_probability),
                    min: temp(d.min),
                    max: temp(d.max),
                    bar_start: (d.min - lo) / range,
                    bar_end: (d.max - lo) / range,
                    rain_total: rain_amount(d.precipitation_sum, true),
                    rain_peak: hours.iter().map(|h: &DayHourView| if h.rain.is_empty() { 0.0 } else { h.rain_height }).fold(0.0, f64::max),
                    has_chance: hours.iter().any(|h: &DayHourView| !h.precipitation.is_empty()),
                    chart,
                    hours,
                }
            })
            .collect()
    }
}

/// "0.4 mm" / "2.5 mm" / "12 mm" (without " mm" if `unit` is false);
/// empty below 0.05 mm, which rounds to nothing.
pub fn rain_amount(mm: f64, unit: bool) -> String {
    if mm < 0.05 {
        return String::new();
    }
    let n = if mm < 10.0 { format!("{mm:.1}") } else { format!("{mm:.0}") };
    if unit { format!("{n} mm") } else { n }
}

/// "40%" when rain is worth mentioning, otherwise empty.
fn rain_chance(p: f64) -> String {
    if p >= 20.0 { format!("{p:.0}%") } else { String::new() }
}

/// "11°", never "-0°".
pub fn temp(t: f64) -> String {
    let r = t.round();
    format!("{}°", if r == 0.0 { 0.0 } else { r })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::Condition;
    use crate::model::Hour;

    fn hour(time: &str, temperature: f64, rain_mm: f64) -> Hour {
        Hour {
            label: format!("{}h", &time[11..13]),
            time: time.into(),
            temperature,
            precipitation_probability: 0.0,
            precipitation: rain_mm,
            condition: Condition::Rain,
            is_day: true,
        }
    }

    fn day(date: &str, min: f64, max: f64, rain: f64) -> Day {
        Day {
            date: date.into(),
            label: "Today".into(),
            min,
            max,
            precipitation_probability: rain,
            precipitation_sum: rain / 10.0,
            sunrise: None,
            sunset: None,
            hours: vec![
                hour(&format!("{date}T00:00"), min, 0.0),
                hour(&format!("{date}T01:00"), max, rain / 20.0),
                hour(&format!("{date}T02:00"), min, 1.0),
            ],
            condition: Condition::Rain,
        }
    }

    #[test]
    fn days_share_the_week_scales() {
        let rows = DayView::list(
            &[day("2026-11-12", 5.0, 10.0, 80.0), day("2026-11-13", 0.0, 20.0, 5.0)],
            Some("2026-11-12T01:00"),
            Lang::En,
        );
        // Temperature bars on the week's range.
        assert_eq!((rows[0].bar_start, rows[0].bar_end), (0.25, 0.5));
        assert_eq!((rows[1].bar_start, rows[1].bar_end), (0.0, 1.0));
        assert_eq!(rows[0].precipitation, "80%");
        assert_eq!(rows[1].precipitation, "");
        assert_eq!(rows[0].max, "10°");
        // Rain: total, and hourly bars on the week's scale (wettest hour 4 mm).
        assert_eq!(rows[0].rain_total, "8.0 mm");
        assert_eq!(rows[0].hours[1].rain, "4.0");
        assert_eq!(rows[0].hours[1].rain_height, 1.0);
        assert_eq!(rows[0].hours[2].rain_height, 0.25);
        assert_eq!(rows[0].hours[0].rain, "");
        // The chart's rain area follows the day's wettest hour.
        assert_eq!(rows[0].rain_peak, 1.0);
        assert_eq!(rows[1].rain_peak, 0.25, "1 mm on a 4 mm scale");
        // The curve: one point per hour, the warmest hour highest.
        assert_eq!(rows[0].chart.points.len(), 3);
        assert!(rows[0].hours[1].y < rows[0].hours[0].y);
        // "Now" in today's chart, which opens there; other days open in the morning.
        assert_eq!(rows[0].hours[1].label, "Now");
        assert!(rows[0].hours[1].is_now && !rows[0].hours[0].is_now);
        assert_eq!(rows[1].hours[1].label, "01h");
        assert_eq!(rows[1].label, "Fri", "2026-11-13");
        assert_eq!(rows[0].first_hour, 1);
        assert_eq!(rows[1].first_hour, 2, "06h, clamped to the hours available");
    }

    #[test]
    fn arrow_points_where_the_wind_goes() {
        // East: the tip (first point) is right of the centre.
        let east = arrow_path(90.0);
        let tip: Vec<f64> = east.split_whitespace().skip(1).take(2).map(|v| v.parse().unwrap()).collect();
        assert_eq!(tip, vec![52.0, 32.0]);
        assert!(arrow_path(0.0).starts_with("M 32.0 12.0"));
    }

    #[test]
    fn formats_rain_amounts() {
        assert_eq!(rain_amount(0.0, true), "");
        assert_eq!(rain_amount(0.04, true), "");
        assert_eq!(rain_amount(0.36, true), "0.4 mm");
        assert_eq!(rain_amount(12.4, false), "12");
    }

    #[test]
    fn rounds_and_avoids_negative_zero() {
        assert_eq!(temp(11.4), "11°");
        assert_eq!(temp(-0.3), "0°");
        assert_eq!(temp(-2.6), "-3°");
    }

}
