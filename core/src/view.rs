//! Ready-to-display data for the main screen.
//!
//! UIs bind these fields directly: no formatting or layout maths in Slint/JS.

use serde::Serialize;

use crate::chart::{ChartGeometry, temperature_chart};
use crate::model::{Day, Forecast};

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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourView {
    pub label: String,
    pub temperature: String,
    pub icon: String,
    /// Empty when rain is unlikely, so the UI shows nothing.
    pub precipitation: String,
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
            days: DayView::list(&f.days),
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
}

impl DayView {
    fn list(days: &[Day]) -> Vec<Self> {
        let lo = days.iter().map(|d| d.min).fold(f64::INFINITY, f64::min);
        let hi = days.iter().map(|d| d.max).fold(f64::NEG_INFINITY, f64::max);
        let range = (hi - lo).max(1.0);
        days.iter()
            .map(|d| Self {
                label: d.label.clone(),
                icon: d.condition.icon(true).into(),
                precipitation: rain_chance(d.precipitation_probability),
                min: temp(d.min),
                max: temp(d.max),
                bar_start: (d.min - lo) / range,
                bar_end: (d.max - lo) / range,
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
            condition: Condition::Rain,
        }
    }

    #[test]
    fn day_bars_share_the_week_scale() {
        let rows = DayView::list(&[day(5.0, 10.0, 80.0), day(0.0, 20.0, 5.0)]);
        assert_eq!((rows[0].bar_start, rows[0].bar_end), (0.25, 0.5));
        assert_eq!((rows[1].bar_start, rows[1].bar_end), (0.0, 1.0));
        assert_eq!(rows[0].precipitation, "80%");
        assert_eq!(rows[1].precipitation, "");
        assert_eq!(rows[0].max, "10°");
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
