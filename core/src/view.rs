//! Ready-to-display data for the main screen.
//!
//! UIs bind these fields directly: no formatting or layout maths in Slint/JS.

use serde::Serialize;

use crate::chart::{ChartGeometry, temperature_chart};
use crate::model::Forecast;

/// Horizontal space per hour in the hourly chart, in logical pixels.
pub const HOUR_STEP: f64 = 60.0;
pub const CHART_HEIGHT: f64 = 150.0;

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
        let chart = temperature_chart(&f.hours, HOUR_STEP, CHART_HEIGHT, 36.0, 14.0);
        let hours = f
            .hours
            .iter()
            .zip(&chart.points)
            .map(|(h, p)| HourView {
                label: h.label.clone(),
                temperature: temp(h.temperature),
                icon: h.condition.icon(h.is_day).into(),
                precipitation: if h.precipitation_probability >= 20.0 {
                    format!("{:.0}%", h.precipitation_probability)
                } else {
                    String::new()
                },
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
        }
    }
}

/// "11°", never "-0°".
pub fn temp(t: f64) -> String {
    let r = t.round();
    format!("{}°", if r == 0.0 { 0.0 } else { r })
}

#[cfg(test)]
mod tests {
    use super::temp;

    #[test]
    fn rounds_and_avoids_negative_zero() {
        assert_eq!(temp(11.4), "11°");
        assert_eq!(temp(-0.3), "0°");
        assert_eq!(temp(-2.6), "-3°");
    }
}
