//! Ready-to-display data for the main screen.
//!
//! UIs bind these fields directly: no formatting or layout maths in Slint/JS.

use serde::Serialize;

use crate::chart::{ChartGeometry, temperature_chart};
use crate::model::{Day, Forecast};
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
    pub icon: String,
    pub high_low: String,
    pub feels_like: String,
    pub humidity: String,
    pub wind: String,
    pub sky_top: u32,
    pub sky_bottom: u32,
    pub sky_top_css: String,
    pub sky_bottom_css: String,
    /// The next hours from now (used by the widgets).
    pub hours: Vec<HourView>,
    pub days: Vec<DayView>,
    /// What the animated background shows.
    pub scene: SceneSpec,
}

/// One of the next hours, for the widgets.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourView {
    pub label: String,
    pub temperature: String,
    pub icon: String,
}

impl MainScreen {
    pub fn from_forecast(f: &Forecast) -> Self {
        let c = &f.current;
        let sky = c.condition.sky(c.is_day);
        let hours = f
            .hours
            .iter()
            .take(HOURS_SHOWN)
            .map(|h| HourView {
                label: h.label.clone(),
                temperature: temp(h.temperature),
                icon: h.condition.icon(h.is_day).into(),
            })
            .collect();
        // The current hour, "15" in "…T15:00", marks "Now" in today's chart.
        let now = f.hours.first().map(|h| h.time.clone());

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
            hours,
            days: DayView::list(&f.days, now.as_deref()),
            scene: SceneSpec::for_weather(c.condition, c.is_day, c.wind_speed, c.wind_direction),
        }
    }
}

/// One row of the 7-day list, and the hour-by-hour chart shown when it's opened.
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
    /// The day's temperature curve (one point per hour).
    pub chart: ChartGeometry,
    pub hours: Vec<DayHourView>,
    /// Hour the chart opens at: now for today, 06h for other days.
    pub first_hour: usize,
}

/// One hour of a day's chart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayHourView {
    /// "15h", or "Now" for the current hour.
    pub label: String,
    pub temperature: String,
    pub icon: String,
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
    fn list(days: &[Day], now: Option<&str>) -> Vec<Self> {
        let lo = days.iter().map(|d| d.min).fold(f64::INFINITY, f64::min);
        let hi = days.iter().map(|d| d.max).fold(f64::NEG_INFINITY, f64::max);
        let range = (hi - lo).max(1.0);
        let wettest = days
            .iter()
            .flat_map(|d| d.hours.iter().map(|h| h.precipitation))
            .fold(RAIN_SCALE_MIN_MM, f64::max);
        days.iter()
            .map(|d| {
                let chart = temperature_chart(&d.hours, HOUR_STEP, CHART_HEIGHT, 30.0, 10.0);
                let now_index = now.and_then(|t| d.hours.iter().position(|h| h.time == t));
                let hours = d
                    .hours
                    .iter()
                    .zip(&chart.points)
                    .enumerate()
                    .map(|(i, (h, p))| DayHourView {
                        label: if Some(i) == now_index { "Now".into() } else { h.label.clone() },
                        temperature: temp(h.temperature),
                        icon: h.condition.icon(h.is_day).into(),
                        precipitation: rain_chance(h.precipitation_probability),
                        rain: rain_amount(h.precipitation, false),
                        rain_height: (h.precipitation / wettest).clamp(0.0, 1.0),
                        x: p.x,
                        y: p.y,
                    })
                    .collect();
                Self {
                    first_hour: now_index.unwrap_or(MORNING.min(d.hours.len().saturating_sub(1))),
                    label: d.label.clone(),
                    icon: d.condition.icon(true).into(),
                    precipitation: rain_chance(d.precipitation_probability),
                    min: temp(d.min),
                    max: temp(d.max),
                    bar_start: (d.min - lo) / range,
                    bar_end: (d.max - lo) / range,
                    rain_total: rain_amount(d.precipitation_sum, true),
                    chart,
                    hours,
                }
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
        // The curve: one point per hour, the warmest hour highest.
        assert_eq!(rows[0].chart.points.len(), 3);
        assert!(rows[0].hours[1].y < rows[0].hours[0].y);
        // "Now" in today's chart, which opens there; other days open in the morning.
        assert_eq!(rows[0].hours[1].label, "Now");
        assert_eq!(rows[1].hours[1].label, "01h");
        assert_eq!(rows[0].first_hour, 1);
        assert_eq!(rows[1].first_hour, 2, "06h, clamped to the hours available");
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
