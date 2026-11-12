//! What the home-screen widgets show, built from the saved cities and the
//! forecast cache, so Kotlin/Swift only lay out ready-made strings.

use std::path::Path;

use serde::Serialize;

use crate::api::{Error, fetch_forecast};
use crate::cache::{self, unix_now};
use crate::places::Places;
use crate::view::{MainScreen, updated_ago};

/// Hours shown in the wide widget.
const WIDGET_HOURS: usize = 5;
/// A cached forecast older than this should be refreshed by the widget.
const MAX_AGE_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WidgetState {
    /// True when there's no cached forecast or it's older than 30 minutes.
    pub needs_refresh: bool,
    /// `None` until the selected city has been downloaded at least once.
    pub view: Option<WidgetView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WidgetView {
    pub city: String,
    /// True for the "📍" device position.
    pub is_device: bool,
    pub icon: String,
    pub temperature: String,
    pub condition: String,
    pub high_low: String,
    /// "Updated 5 min ago".
    pub updated: String,
    /// Sky gradient, as 0xRRGGBB.
    pub sky_top: u32,
    pub sky_bottom: u32,
    pub hours: Vec<WidgetHour>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WidgetHour {
    pub label: String,
    pub icon: String,
    pub temperature: String,
}

/// The widget's content for the selected city, from the files in `dir`.
pub fn widget_state(dir: &Path, unix_now: i64) -> WidgetState {
    let places = Places::load(dir);
    let city = places.selected();
    let Some(cached) = cache::load(dir, city, unix_now) else {
        return WidgetState { needs_refresh: true, view: None };
    };
    let age = unix_now - cached.fetched_at;
    let s = MainScreen::from_forecast(&cached.forecast);
    WidgetState {
        needs_refresh: age > MAX_AGE_SECS,
        view: Some(WidgetView {
            city: s.city,
            is_device: city.is_device,
            icon: s.icon,
            temperature: s.temperature,
            condition: s.condition,
            high_low: s.high_low,
            updated: updated_ago(age),
            sky_top: s.sky_top,
            sky_bottom: s.sky_bottom,
            hours: s
                .hours
                .into_iter()
                .take(WIDGET_HOURS)
                .map(|h| WidgetHour { label: h.label, icon: h.icon, temperature: h.temperature })
                .collect(),
        }),
    }
}

/// Downloads the selected city's forecast into the cache (for widgets that
/// refresh while the app is closed).
pub async fn refresh_selected(dir: &Path) -> Result<(), Error> {
    let city = Places::load(dir).selected().clone();
    let forecast = fetch_forecast(&city).await?;
    cache::save(dir, &city, &forecast, unix_now()).map_err(Error::Cache)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::parse_forecast;
    use crate::model::Location;

    const SAMPLE: &str = r#"{
        "utc_offset_seconds": 3600,
        "current": {"time": "2026-11-12T14:15", "temperature_2m": 11.2, "apparent_temperature": 9.1,
                    "relative_humidity_2m": 78, "is_day": 1, "weather_code": 0, "wind_speed_10m": 14.4},
        "hourly": {
            "time": ["2026-11-12T14:00", "2026-11-12T15:00", "2026-11-12T16:00", "2026-11-12T17:00",
                     "2026-11-12T18:00", "2026-11-12T19:00", "2026-11-12T20:00"],
            "temperature_2m": [11.2, 11.5, 11.0, 10.0, 9.0, 8.4, 8.0],
            "precipitation_probability": [0, 0, 0, 0, 0, 0, 0],
            "weather_code": [0, 0, 1, 2, 3, 3, 3],
            "is_day": [1, 1, 1, 1, 0, 0, 0]
        },
        "daily": {"time": ["2026-11-12"], "weather_code": [2],
                  "temperature_2m_max": [12.4], "temperature_2m_min": [7.9],
                  "precipitation_probability_max": [10]}
    }"#;
    const FETCHED_AT: i64 = 1_794_489_300; // 14:15 in Paris

    #[test]
    fn builds_state_for_selected_city() {
        let dir = std::env::temp_dir().join(format!("meteo-widget-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        // Nothing downloaded yet.
        let empty = widget_state(&dir, FETCHED_AT);
        assert_eq!(empty, WidgetState { needs_refresh: true, view: None });

        let forecast = parse_forecast("Paris", SAMPLE.as_bytes()).unwrap();
        cache::save(&dir, &Location::paris(), &forecast, FETCHED_AT).unwrap();

        let fresh = widget_state(&dir, FETCHED_AT + 60);
        assert!(!fresh.needs_refresh);
        let v = fresh.view.unwrap();
        assert_eq!((v.city.as_str(), v.temperature.as_str()), ("Paris", "11°"));
        assert_eq!(v.updated, "Updated 1 min ago");
        assert_eq!(v.hours.len(), WIDGET_HOURS);
        assert_eq!(v.hours[0].label, "Now");

        // An hour later: still shown, but due for a refresh, and moved on.
        let later = widget_state(&dir, FETCHED_AT + 3600);
        assert!(later.needs_refresh);
        assert_eq!(later.view.unwrap().hours[1].label, "16h");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
