//! Ready-to-display data for the main screen.
//!
//! UIs bind these fields directly: no formatting or layout maths in Slint/JS.

use serde::Serialize;

use crate::chart::{ChartGeometry, temperature_chart};
use crate::model::{Day, Forecast};
use crate::scene::SceneSpec;

/// Horizontal space per hour in the hourly chart, in logical pixels.
pub const HOUR_STEP: f64 = 60.0;
pub const CHART_HEIGHT: f64 = 150.0;
/// Hours shown in the hourly chart.
pub const HOURS_SHOWN: usize = 24;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MainScreen {
    pub city: String,
    pub temperature: String,
    pub condition: String,
    pub icon: String,
    pub high_low: String,
    pub feels_like: String,
    pub humidity: String,
    pub wind: String,
    pub sky_top: u32,
    pub sky_bottom: u32,
    pub sky_top_css: String,
    pub sky_bottom_css: String,
    pub chart: ChartGeometry,
    pub hours: Vec<HourView>,
    pub days: Vec<DayView>,
    /// What the animated background shows.
    pub scene: SceneSpec,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourView {
    pub label: String,
    pub temperature: String,
    pub icon: String,
    /// Empty when rain is unlikely, so the UI shows nothing.
    pub precipitation: String,
    /// Expected amount, e.g. "0.4 mm"; empty when dry.
    pub rain: String,
    /// Centre of this hour's point on the chart.
    pub x: f64,
    pub y: f64,
}

impl MainScreen {
    pub fn from_forecast(f: &Forecast) -> Self {
        let c = &f.current;
        let sky = c.condition.sky(c.is_day);
        let shown = &f.hours[..f.hours.len().min(HOURS_SHOWN)];
        let chart = temperature_chart(shown, HOUR_STEP, CHART_HEIGHT, 36.0, 14.0);
        let hours = shown
            .iter()
            .zip(&chart.points)
            .map(|(h, p)| HourView {
                label: h.label.clone(),
                temperature: temp(h.temperature),
                icon: h.condition.icon(h.is_day).into(),
                precipitation: rain_chance(h.precipitation_probability),
                rain: rain_amount(h.precipitation, true),
                x: p.x,
                y: p.y,
            })
            .collect();

        Self {
            city: f.location.clone(),
            temperature: temp(c.temperature),
            condition: c.condition.label().into(),
            icon: c.condition.icon(c.is_day).into(),
            high_low: format!("H {}  L {}", temp(f.today_max), temp(f.today_min)),
            feels_like: temp(c.apparent_temperature),
            humidity: format!("{:.0}%", c.humidity),
            wind: format!("{:.0} km/h", c.wind_speed),
            sky_top: sky.top,
            sky_bottom: sky.bottom,
            sky_top_css: sky.top_css(),
            sky_bottom_css: sky.bottom_css(),
            chart,
            hours,
            days: DayView::list(&f.days, f.hours.first().and_then(|h| h.time.get(11..13)?.parse().ok())),
            scene: SceneSpec::for_weather(c.condition, c.is_day, c.wind_speed, c.wind_direction),
        }
    }
}

/// One row of the 7-day list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayView {
    pub label: String,
    pub icon: String,
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
    /// One bar per hour of the day, for the day's rain detail.
    pub rain: Vec<RainBar>,
    /// Hour the rain detail opens at: now for today, otherwise the first
    /// wet hour, so the interesting part is in view.
    pub first_hour: usize,
}

/// One hour in a day's rain chart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RainBar {
    /// "00h" … "23h".
    pub label: String,
    /// "1.2"; empty when dry.
    pub amount: String,
    /// Bar height, 0–1, on a scale shared by the whole week.
    pub height: f64,
}

/// Bars are scaled to the wettest hour of the week, but never so that a
/// drizzle fills the chart.
const RAIN_SCALE_MIN_MM: f64 = 2.0;

impl DayView {
    /// `now_hour`: the current hour of the day, used for today's detail.
    fn list(days: &[Day], now_hour: Option<usize>) -> Vec<Self> {
        let lo = days.iter().map(|d| d.min).fold(f64::INFINITY, f64::min);
        let hi = days.iter().map(|d| d.max).fold(f64::NEG_INFINITY, f64::max);
        let range = (hi - lo).max(1.0);
        let wettest = days.iter().flat_map(|d| d.rain.iter().copied()).fold(RAIN_SCALE_MIN_MM, f64::max);
        days.iter()
            .enumerate()
            .map(|(i, d)| Self {
                first_hour: match (i, now_hour) {
                    (0, Some(now)) => now,
                    _ => d.rain.iter().position(|&mm| mm >= 0.05).unwrap_or(0),
                },
                label: d.label.clone(),
                icon: d.condition.icon(true).into(),
                precipitation: rain_chance(d.precipitation_probability),
                min: temp(d.min),
                max: temp(d.max),
                bar_start: (d.min - lo) / range,
                bar_end: (d.max - lo) / range,
                rain_total: rain_amount(d.precipitation_sum, true),
                rain: d
                    .rain
                    .iter()
                    .enumerate()
                    .map(|(hour, &mm)| RainBar {
                        label: format!("{hour:02}h"),
                        amount: rain_amount(mm, false),
                        height: (mm / wettest).clamp(0.0, 1.0),
                    })
                    .collect(),
            })
            .collect()
    }
}

/// "Updated 2 h ago", for a forecast downloaded `age_secs` seconds ago.
pub fn updated_ago(age_secs: i64) -> String {
    let age = age_secs.max(0);
    match age {
        0..60 => "Updated just now".into(),
        60..3600 => format!("Updated {} min ago", age / 60),
        3600..86_400 => format!("Updated {} h ago", age / 3600),
        86_400..172_800 => "Updated yesterday".into(),
        _ => format!("Updated {} days ago", age / 86_400),
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

    fn day(min: f64, max: f64, rain: f64) -> Day {
        Day {
            date: "2026-11-12".into(),
            label: "Today".into(),
            min,
            max,
            precipitation_probability: rain,
            precipitation_sum: rain / 10.0,
            rain: vec![0.0, rain / 20.0, 1.0],
            condition: Condition::Rain,
        }
    }

    #[test]
    fn day_bars_share_the_week_scale() {
        let rows = DayView::list(&[day(5.0, 10.0, 80.0), day(0.0, 20.0, 5.0)], Some(14));
        assert_eq!((rows[0].bar_start, rows[0].bar_end), (0.25, 0.5));
        assert_eq!((rows[1].bar_start, rows[1].bar_end), (0.0, 1.0));
        assert_eq!(rows[0].precipitation, "80%");
        assert_eq!(rows[1].precipitation, "");
        assert_eq!(rows[0].max, "10°");
        // Rain: total, and hourly bars on the week's scale (wettest hour 4 mm).
        assert_eq!(rows[0].rain_total, "8.0 mm");
        assert_eq!(rows[0].rain[1].amount, "4.0");
        assert_eq!(rows[0].rain[1].height, 1.0);
        assert_eq!(rows[0].rain[2].height, 0.25);
        assert_eq!(rows[0].rain[0].amount, "");
        assert_eq!(rows[1].rain[1].label, "01h");
        // Today opens at the current hour, other days at their first wet hour.
        assert_eq!(rows[0].first_hour, 14);
        assert_eq!(rows[1].first_hour, 1);
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

    #[test]
    fn describes_cache_age() {
        assert_eq!(updated_ago(-5), "Updated just now");
        assert_eq!(updated_ago(59), "Updated just now");
        assert_eq!(updated_ago(150), "Updated 2 min ago");
        assert_eq!(updated_ago(3 * 3600 + 59), "Updated 3 h ago");
        assert_eq!(updated_ago(30 * 3600), "Updated yesterday");
        assert_eq!(updated_ago(3 * 86_400), "Updated 3 days ago");
    }
}
