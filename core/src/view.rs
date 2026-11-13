//! Ready-to-display data for the main screen.
//!
//! UIs bind these fields directly: no formatting or layout maths in Slint/JS.

use serde::Serialize;

use crate::chart::{ChartGeometry, temperature_chart_with_band};
use crate::date::weekday_index;
use crate::extras::{Air, rain_agreement, spread_by_time, temperature_agreement};
use crate::i18n::Lang;
use crate::landscape::{Landscape, Phase, landscape, seed_for};
use crate::model::{Day, Forecast};
use crate::palette::{Palette, icon_kind};
use crate::scene::SceneSpec;
use crate::units::Units;

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
    pub nowcast: NowcastView,
    /// Official warnings in force or coming, strongest first.
    pub warnings: Vec<WarningView>,
    pub air: AirView,
    /// "3° warmer than usual"; empty without normals.
    pub vs_normal: String,
    /// Current rain rate, mm/h, for the animation.
    pub rain_rate: f64,
}

/// Rain over the next 2 hours, in 15-minute bars.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NowcastView {
    pub summary: String,
    pub bars: Vec<NowBar>,
    /// Any rain in the next 2 hours.
    pub wet: bool,
    /// Minutes until rain starts (0 if raining now); None if dry.
    pub starts_in: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NowBar {
    /// "Now", "+30 min", … on every other bar; empty otherwise.
    pub label: String,
    /// 0–1 (8 mm/h fills it).
    pub height: f64,
    pub wet: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AirView {
    /// "28"; "--" if unknown.
    pub aqi: String,
    pub aqi_level: String,
    /// The strongest pollen ("Birch"), or a note when there's none.
    pub pollen: String,
    pub pollen_level: String,
    /// The next days at a glance, "Sat moderate · Sun high · Mon low"; empty
    /// when no pollen is expected or there's no forecast.
    pub pollen_outlook: String,
}

impl NowcastView {
    fn new(f: &Forecast, lang: Lang) -> Self {
        let q = &f.nowcast;
        if q.is_empty() {
            return Self::default();
        }
        const WET: f64 = 0.03; // mm in 15 min
        let rate = |mm: f64| mm * 4.0;
        let bars = q
            .iter()
            .enumerate()
            .map(|(i, step)| NowBar {
                label: if i % 2 == 0 { lang.in_minutes(i as u32 * 15) } else { String::new() },
                height: (rate(step.precipitation) / 8.0).clamp(0.0, 1.0),
                wet: step.precipitation >= WET,
            })
            .collect();
        let first_wet = q.iter().position(|s| s.precipitation >= WET);
        let peak = q.iter().map(|s| rate(s.precipitation)).fold(0.0, f64::max);
        let word = lang.rain_word(peak);
        let summary = match first_wet {
            None => lang.no_rain_soon().to_string(),
            Some(0) => match q.iter().position(|s| s.precipitation < WET) {
                Some(dry) => lang.rain_stopping(word, &clock(&q[dry].time)),
                None => lang.rain_lasting(word),
            },
            Some(k) => lang.rain_starting(word, k as u32 * 15),
        };
        Self { summary, bars, wet: first_wet.is_some(), starts_in: first_wet.map(|k| k as u32 * 15) }
    }
}

/// "10:15" from "…T10:15".
fn clock(time: &str) -> String {
    time.get(11..16).unwrap_or("").to_string()
}

/// Pollen thresholds (grains/m³) for moderate, high, very high.
fn pollen_thresholds(kind: &str) -> [f64; 3] {
    match kind {
        "grass" | "ragweed" | "mugwort" => [5.0, 20.0, 50.0],
        _ => [10.0, 50.0, 200.0],
    }
}

impl AirView {
    fn new(air: Option<&Air>, lang: Lang) -> Self {
        let Some(air) = air else {
            return Self {
                aqi: "--".into(),
                pollen: "--".into(),
                pollen_level: lang.not_here().into(),
                aqi_level: lang.not_here().into(),
                pollen_outlook: String::new(),
            };
        };
        let (aqi, aqi_level) = match air.european_aqi {
            Some(a) => (format!("{a:.0}"), lang.aqi_level(a).to_string()),
            None => ("--".into(), lang.not_here().into()),
        };
        // The pollen with the highest level (relative to its own scale).
        let level = |kind: &str, v: f64| pollen_thresholds(kind).iter().filter(|&&t| v >= t).count();
        let top = air.pollen.iter().filter(|(_, v)| *v >= 1.0).max_by(|a, b| {
            (level(&a.0, a.1), a.1).partial_cmp(&(level(&b.0, b.1), b.1)).unwrap_or(std::cmp::Ordering::Equal)
        });
        let (pollen, pollen_level) = match (top, air.pollen.is_empty()) {
            (_, true) => ("--".into(), lang.not_here().into()),
            (None, false) => ("–".into(), lang.no_pollen().into()),
            (Some((kind, v)), false) => (lang.pollen_name(kind).into(), lang.pollen_level(level(kind, *v)).into()),
        };
        // The coming days (after today): each day's strongest pollen level.
        let days: Vec<(String, Option<usize>)> = air
            .pollen_days
            .iter()
            .skip(1)
            .map(|d| {
                let label = weekday_index(&d.date).map_or_else(|| d.date.clone(), |w| lang.weekday(w).to_string());
                let strongest = d.pollen.iter().filter(|(_, v)| *v >= 1.0).map(|(k, v)| level(k, *v)).max();
                (label, strongest)
            })
            .collect();
        let pollen_outlook = if days.iter().any(|(_, l)| l.is_some()) {
            days.iter()
                .map(|(day, l)| {
                    let word = l.map_or(lang.no_pollen_short(), |l| lang.pollen_level(l));
                    format!("{day} {}", word.to_lowercase())
                })
                .collect::<Vec<_>>()
                .join(" · ")
        } else {
            String::new()
        };
        Self { aqi, aqi_level, pollen, pollen_level, pollen_outlook }
    }
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
/// A weather warning, ready to show.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WarningView {
    /// "Orange warning · Thunderstorms".
    pub title: String,
    /// "Until 23:00", "From Sat 06:00 to Sun 12:00".
    pub detail: String,
    /// 1 yellow, 2 orange, 3 red.
    pub level: u8,
}

impl WarningView {
    fn list(f: &Forecast, lang: Lang) -> Vec<Self> {
        // The forecast's local time, back to Unix seconds.
        let Some(now) = crate::date::parse_rfc3339(&f.current.time).map(|t| t - f.utc_offset_seconds) else {
            return Vec::new();
        };
        let today = f.current.time.get(..10).unwrap_or("");
        // "23:00" today, "midnight" tonight, "Sat 06:00" another day.
        let at = |unix: i64| {
            let local = crate::date::local_iso_minute(unix, f.utc_offset_seconds);
            let clock = local.get(11..16).unwrap_or("").to_string();
            if clock == "00:00"
                && crate::date::local_iso_minute(unix - 60, f.utc_offset_seconds).get(..10) == Some(today)
            {
                return lang.midnight().to_string();
            }
            match local.get(..10) {
                Some(day) if day != today => {
                    weekday_index(day).map_or(clock.clone(), |w| format!("{} {clock}", lang.weekday(w)))
                }
                _ => clock,
            }
        };
        f.warnings
            .iter()
            .filter(|w| w.is_current(now))
            .map(|w| Self {
                title: lang.warning_title(w.level, &w.kind),
                detail: lang.warning_when((w.onset > now).then(|| at(w.onset)).as_deref(), &at(w.expires)),
                level: w.level,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetailsView {
    pub feels_like: String,
    pub feels_note: String,
    pub humidity: String,
    pub dew_point: String,
    /// "20", with the unit apart so the number can be big.
    pub wind_speed: String,
    pub wind_unit: String,
    /// "Gusts 45 km/h"; empty if unknown.
    pub wind_gusts: String,
    /// "From the SW".
    pub wind_from: String,
    /// An arrow pointing where the wind blows *to*, drawn in a 64×64 compass
    /// box (centre 32,32).
    pub wind_arrow_path: String,
    /// "07:42"; empty if unknown.
    pub sunrise: String,
    pub sunset: String,
    pub daylight: String,
    /// "3"; "--" if unknown.
    pub uv: String,
    pub uv_level: String,
    /// "1013 hPa"; "--" if unknown.
    pub pressure: String,
    pub pressure_note: String,
}

impl MainScreen {
    pub fn from_forecast(f: &Forecast, lang: Lang, units: Units) -> Self {
        let c = &f.current;
        let sky = c.condition.sky(c.is_day);
        let hours = f
            .hours
            .iter()
            .take(HOURS_SHOWN)
            .enumerate()
            .map(|(i, h)| HourView {
                label: if i == 0 { lang.now().into() } else { h.label.clone() },
                temperature: units.temp(h.temperature),
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
            temperature: units.temp(c.temperature),
            condition: lang.condition(c.condition).into(),
            icon: c.condition.icon(c.is_day).into(),
            icon_kind: icon_kind(c.condition, c.is_day).into(),
            high_low: lang.day_night(&units.temp(f.today_max), &units.temp(f.today_min)),
            feels_like: lang.feels_like(&units.temp(c.apparent_temperature)),
            sky_top: sky.top,
            sky_bottom: sky.bottom,
            sky_top_css: sky.top_css(),
            sky_bottom_css: sky.bottom_css(),
            hours,
            days: DayView::list(&f.days, &f.spread, now.as_deref(), lang, units),
            details: DetailsView::new(f, rise, set, lang, units),
            scene: SceneSpec::for_weather(c.condition, c.is_day, c.wind_speed, c.wind_direction)
                .with_rain_rate(c.precipitation.map_or(0.0, |mm| mm * 4.0)),
            landscape: land,
            palette,
            nowcast: NowcastView::new(f, lang),
            warnings: WarningView::list(f, lang),
            air: AirView::new(f.air.as_ref(), lang),
            vs_normal: f
                .days
                .first()
                .and_then(|d| Some(d.max - d.normal_max?))
                .map(|diff| lang.vs_normal(diff, &units.temp_delta(diff)))
                .unwrap_or_default(),
            rain_rate: c.precipitation.map_or(0.0, |mm| mm * 4.0),
        }
    }
}

impl DetailsView {
    fn new(f: &Forecast, rise: Option<&str>, set: Option<&str>, lang: Lang, units: Units) -> Self {
        let c = &f.current;
        let clock = |t: Option<&str>| t.and_then(|t| t.get(11..16)).unwrap_or("").to_string();
        let minutes = |t: &str| -> Option<i64> {
            Some(t.get(11..13)?.parse::<i64>().ok()? * 60 + t.get(14..16)?.parse::<i64>().ok()?)
        };
        let daylight = match (rise.and_then(minutes), set.and_then(minutes)) {
            (Some(r), Some(s)) if s > r => lang.daylight(s - r),
            _ => String::new(),
        };
        Self {
            feels_like: units.temp(c.apparent_temperature),
            feels_note: lang.feels_note(c.temperature, c.apparent_temperature).into(),
            humidity: format!("{:.0}%", c.humidity),
            dew_point: c.dew_point.map(|d| lang.dew_point(&units.temp(d))).unwrap_or_default(),
            wind_speed: units.wind(c.wind_speed).0,
            wind_unit: units.wind(c.wind_speed).1.into(),
            wind_gusts: c.wind_gusts.map(|g| lang.gusts(&units.wind_text(g))).unwrap_or_default(),
            wind_from: lang.wind_from(c.wind_direction),
            wind_arrow_path: arrow_path((c.wind_direction + 180.0).rem_euclid(360.0)),
            sunrise: clock(rise),
            sunset: clock(set),
            daylight,
            uv: c.uv_index.map_or("--".into(), |u| format!("{:.0}", u.round())),
            uv_level: c.uv_index.map_or_else(|| lang.not_available().to_string(), |u| lang.uv_level(u).to_string()),
            pressure: c.pressure.map_or("--".into(), |p| units.pressure(p)),
            pressure_note: c
                .pressure
                .map_or_else(|| lang.not_available().to_string(), |p| lang.pressure_level(p).to_string()),
        }
    }
}

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
    /// "5 models agree · temperature 91% · rain 76%"; empty without the
    /// models' data.
    pub agreement: String,
    /// Whether some hour shows the models' range of rain.
    pub has_range: bool,
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
    /// The models' range of rain this hour, "0.4–2.1"; empty when they
    /// agree (or all say dry).
    pub rain_range: String,
    /// That range as heights (0–1) on the same scale as `rain_height`.
    pub range_low: f64,
    pub range_high: f64,
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
    fn list(days: &[Day], spread: &[crate::extras::Spread], now: Option<&str>, lang: Lang, units: Units) -> Vec<Self> {
        let by_time = spread_by_time(spread);
        let lo = days.iter().map(|d| d.min).fold(f64::INFINITY, f64::min);
        let hi = days.iter().map(|d| d.max).fold(f64::NEG_INFINITY, f64::max);
        let range = (hi - lo).max(1.0);
        let wettest = days
            .iter()
            .flat_map(|d| d.hours.iter().map(|h| h.precipitation))
            .chain(spread.iter().map(|s| s.r_max))
            .fold(RAIN_SCALE_MIN_MM, f64::max);
        days.iter()
            .enumerate()
            .map(|(day_index, d)| {
                let day_spread: Vec<_> = d.hours.iter().map(|h| by_time.get(h.time.as_str()).copied()).collect();
                let band: Vec<_> = day_spread.iter().map(|s| s.map(|s| (s.t_min, s.t_max))).collect();
                let chart = temperature_chart_with_band(&d.hours, &band, HOUR_STEP, CHART_HEIGHT, 30.0, 10.0);
                let known: Vec<_> = day_spread.iter().flatten().copied().collect();
                let models = known.iter().map(|s| s.models as usize).max().unwrap_or(0);
                let agreement = match (temperature_agreement(&known), rain_agreement(&known)) {
                    (Some(t), Some(r)) => lang.models_agree(t, r, models),
                    _ => String::new(),
                };
                let now_index = now.and_then(|t| d.hours.iter().position(|h| h.time == t));
                let hours: Vec<DayHourView> = d
                    .hours
                    .iter()
                    .zip(&chart.points)
                    .zip(&day_spread)
                    .enumerate()
                    .map(|(i, ((h, p), s))| {
                        let range = s.filter(|s| rain_differs(s.r_min, s.r_max));
                        DayHourView {
                            label: if Some(i) == now_index { lang.now().into() } else { h.label.clone() },
                            is_now: Some(i) == now_index,
                            temperature: units.temp(h.temperature),
                            icon: h.condition.icon(h.is_day).into(),
                            icon_kind: icon_kind(h.condition, h.is_day).into(),
                            precipitation: rain_chance(h.precipitation_probability),
                            rain: units.rain(h.precipitation, false),
                            rain_height: (h.precipitation / wettest).clamp(0.0, 1.0),
                            rain_range: range.map(|s| rain_range(units, s.r_min, s.r_max)).unwrap_or_default(),
                            range_low: range.map_or(0.0, |s| (s.r_min / wettest).clamp(0.0, 1.0)),
                            range_high: range.map_or(0.0, |s| (s.r_max / wettest).clamp(0.0, 1.0)),
                            x: p.x,
                            y: p.y,
                        }
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
                    min: units.temp(d.min),
                    max: units.temp(d.max),
                    bar_start: (d.min - lo) / range,
                    bar_end: (d.max - lo) / range,
                    rain_total: units.rain(d.precipitation_sum, true),
                    rain_peak: hours
                        .iter()
                        .map(|h: &DayHourView| if h.rain.is_empty() { 0.0 } else { h.rain_height }.max(h.range_high))
                        .fold(0.0, f64::max),
                    has_range: hours.iter().any(|h: &DayHourView| !h.rain_range.is_empty()),
                    has_chance: hours.iter().any(|h: &DayHourView| !h.precipitation.is_empty()),
                    agreement,
                    chart,
                    hours,
                }
            })
            .collect()
    }
}

/// Whether the models' rain this hour is worth showing as a range: some
/// expect at least 0.1 mm, and they're at least 0.3 mm apart.
fn rain_differs(low: f64, high: f64) -> bool {
    high >= 0.1 && high - low >= 0.3
}

/// "0–1.2", "0.4–2.1" (no unit; the chart's amounts have none).
fn rain_range(units: Units, low: f64, high: f64) -> String {
    let low = units.rain(low, false);
    let low = if low.is_empty() { "0".to_string() } else { low };
    format!("{low}–{}", units.rain(high, false))
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
            normal_max: None,
            normal_min: None,
            hours: vec![
                hour(&format!("{date}T00:00"), min, 0.0),
                hour(&format!("{date}T01:00"), max, rain / 20.0),
                hour(&format!("{date}T02:00"), min, 1.0),
            ],
            condition: Condition::Rain,
        }
    }

    #[test]
    fn pollen_outlook_for_the_next_days() {
        use crate::extras::{Air, PollenDay};
        let day = |date: &str, birch: f64| PollenDay { date: date.into(), pollen: vec![("birch".into(), birch)] };
        let air = Air {
            european_aqi: Some(20.0),
            pm2_5: None,
            pollen: vec![("birch".into(), 5.0)],
            // Fri (today), Sat, Sun, Mon.
            pollen_days: vec![
                day("2026-11-13", 5.0),
                day("2026-11-14", 30.0),
                day("2026-11-15", 0.0),
                day("2026-11-16", 250.0),
            ],
        };
        let v = AirView::new(Some(&air), Lang::En);
        assert_eq!(v.pollen_level, "Low");
        assert_eq!(v.pollen_outlook, "Sat moderate · Sun none · Mon very high");
        // Nothing coming: no outlook.
        let quiet = Air { pollen_days: vec![day("2026-11-13", 0.0), day("2026-11-14", 0.0)], ..air };
        assert_eq!(AirView::new(Some(&quiet), Lang::Fr).pollen_outlook, "");
    }

    #[test]
    fn warnings_read_like_a_sentence() {
        let json = br#"{"utc_offset_seconds":3600,"current":{"time":"2026-11-13T14:00","temperature_2m":7,"apparent_temperature":5,"relative_humidity_2m":80,"is_day":1,"weather_code":3,"wind_speed_10m":10},
            "hourly":{"time":["2026-11-13T14:00"],"temperature_2m":[7],"weather_code":[3],"is_day":[1]},
            "daily":{"time":["2026-11-13"],"weather_code":[3],"temperature_2m_max":[10],"temperature_2m_min":[5],"precipitation_sum":[0]}}"#;
        let mut f = crate::api::parse_forecast("Grenoble", json).unwrap();
        // 14:00 local is 13:00 UTC = 1_794_574_800.
        let now = 1_794_574_800;
        let w = |level, onset, expires| crate::warnings::Warning {
            kind: "thunderstorm".into(),
            level,
            area: "Isère".into(),
            onset,
            expires,
            id: String::new(),
        };
        f.warnings = vec![
            w(2, now - 3600, now + 10 * 3600),      // until 00:00 local
            w(1, now + 4 * 3600, now + 6 * 3600),   // 18:00 to 20:00
            w(3, now + 20 * 3600, now + 30 * 3600), // tomorrow
            w(1, now - 7200, now - 3600),           // over
        ];
        let screen = MainScreen::from_forecast(&f, Lang::En, Units::Metric);
        let lines: Vec<_> = screen.warnings.iter().map(|w| format!("{} | {}", w.title, w.detail)).collect();
        assert_eq!(
            lines,
            [
                "Orange warning · Thunderstorms | Until midnight",
                "Yellow warning · Thunderstorms | From 18:00 to 20:00",
                "Red warning · Thunderstorms | From Sat 10:00 to Sat 20:00",
            ]
        );
    }

    #[test]
    fn models_rain_range() {
        assert!(!rain_differs(0.0, 0.05), "all dry");
        assert!(!rain_differs(1.0, 1.2), "close enough");
        assert!(rain_differs(0.0, 1.2));
        assert_eq!(rain_range(Units::Metric, 0.0, 1.2), "0–1.2");
        assert_eq!(rain_range(Units::Metric, 0.4, 12.3), "0.4–12");
        assert_eq!(rain_range(Units::Imperial, 2.54, 25.4), "0.10–1.00");
    }

    #[test]
    fn days_share_the_week_scales() {
        let rows = DayView::list(
            &[day("2026-11-12", 5.0, 10.0, 80.0), day("2026-11-13", 0.0, 20.0, 5.0)],
            &[],
            Some("2026-11-12T01:00"),
            Lang::En,
            Units::Metric,
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
    fn next_two_hours() {
        use crate::model::Quarter;
        let q = |mm: &[f64]| -> Vec<Quarter> {
            mm.iter()
                .enumerate()
                .map(|(i, &p)| Quarter {
                    time: format!("2026-11-13T{:02}:{:02}", 9 + i / 4, i % 4 * 15),
                    precipitation: p,
                })
                .collect()
        };
        let view = |mm: &[f64]| {
            let mut f = crate::api::parse_forecast("X", br#"{"current":{"time":"2026-11-13T09:00","temperature_2m":1,"apparent_temperature":1,"relative_humidity_2m":1,"is_day":1,"weather_code":0,"wind_speed_10m":1},"hourly":{"time":["2026-11-13T09:00"],"temperature_2m":[1],"weather_code":[0],"is_day":[1]},"daily":{"time":["2026-11-13"],"weather_code":[0],"temperature_2m_max":[1],"temperature_2m_min":[0]}}"#).unwrap();
            f.nowcast = q(mm);
            NowcastView::new(&f, Lang::En)
        };
        assert_eq!(view(&[0.0; 8]).summary, "No rain expected in the next 2 hours");
        let soon = view(&[0.0, 0.0, 0.1, 0.2, 0.2, 0.0, 0.0, 0.0]);
        assert_eq!(soon.summary, "Light rain starting in about 30 min");
        assert_eq!(soon.starts_in, Some(30));
        assert_eq!(view(&[1.5, 1.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]).summary, "Heavy rain, stopping around 09:30");
        assert_eq!(view(&[0.5; 8]).summary, "Rain for at least 2 hours");
        assert_eq!(soon.bars[2].label, "+30 min");
        assert!(view(&[]).summary.is_empty());
    }

    #[test]
    fn compared_with_normal() {
        assert_eq!(Lang::En.vs_normal(3.2, &Units::Metric.temp_delta(3.2)), "3° warmer than usual");
        assert_eq!(Lang::Fr.vs_normal(-2.0, &Units::Metric.temp_delta(-2.0)), "2° de moins que la normale");
        assert_eq!(Lang::En.vs_normal(0.4, "0°"), "Normal for the date");
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
