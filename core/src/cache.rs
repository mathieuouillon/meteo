//! The last successful forecast, saved as JSON so the app (and later the
//! home-screen widgets) can show something instantly and when offline.

use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::{Forecast, Location};
use crate::source::Source;

/// One file per city and source, so switching either works offline too.
/// (The default source keeps the plain name used before sources existed.)
fn file_name(location: &Location, source: Source) -> String {
    match source {
        Source::BestMatch => format!("forecast-{}.json", location.key()),
        other => format!("forecast-{}-{}.json", location.key(), other.model_id()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedForecast {
    /// When the forecast was downloaded, as a Unix timestamp.
    pub fetched_at: i64,
    pub forecast: Forecast,
}

/// Seconds since the Unix epoch.
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Saves the forecast for `location` in `dir` (created if needed).
pub fn save(dir: &Path, location: &Location, source: Source, forecast: &Forecast, fetched_at: i64) -> io::Result<()> {
    let json = serde_json::to_vec(&CachedForecast {
        fetched_at,
        forecast: forecast.clone(),
    })?;
    write_atomic(dir, &file_name(location, source), &json)
}

/// Writes a temporary file and renames it, so a reader (another thread, or
/// a home-screen widget) never sees a half-written file.
pub(crate) fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{name}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, dir.join(name))
}

/// Loads the cached forecast for `location` from `dir`, moved forward to `unix_now`.
/// Returns `None` if there is no cache, it can't be read (e.g. written by an
/// older version), or it is so old that no upcoming hours are left.
pub fn load(dir: &Path, location: &Location, source: Source, unix_now: i64) -> Option<CachedForecast> {
    let bytes = std::fs::read(dir.join(file_name(location, source))).ok()?;
    let mut cached: CachedForecast = serde_json::from_slice(&bytes).ok()?;
    cached.forecast.advance_to(unix_now);
    let usable = !cached.forecast.hours.is_empty() && !cached.forecast.days.is_empty();
    usable.then_some(cached)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::parse_forecast;

    const SAMPLE: &str = r#"{
        "utc_offset_seconds": 3600,
        "current": {"time": "2026-11-12T14:15", "temperature_2m": 11.2, "apparent_temperature": 9.1,
                    "relative_humidity_2m": 78, "is_day": 1, "weather_code": 61, "wind_speed_10m": 14.4},
        "hourly": {
            "time": ["2026-11-12T14:00", "2026-11-12T15:00", "2026-11-12T16:00", "2026-11-13T00:00"],
            "temperature_2m": [11.2, 11.5, 11.0, 7.0],
            "precipitation_probability": [40, 50, 70, 0],
            "weather_code": [61, 63, 63, 0],
            "is_day": [1, 1, 1, 0]
        },
        "daily": {"time": ["2026-11-12", "2026-11-13", "2026-11-14"], "weather_code": [61, 0, 3],
                  "temperature_2m_max": [12.4, 14.0, 9.0], "temperature_2m_min": [7.9, 6.5, 3.0],
                  "precipitation_probability_max": [80, null, 10]}
    }"#;

    fn paris() -> Location {
        Location::paris()
    }

    /// 2026-11-12 13:15 UTC = 14:15 in Paris.
    const FETCHED_AT: i64 = 1_794_489_300;
    const HOUR: i64 = 3600;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("meteo-cache-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trips_and_advances_to_now() {
        let dir = temp_dir("roundtrip");
        let forecast = parse_forecast("Paris", SAMPLE.as_bytes()).unwrap();
        save(&dir, &paris(), Source::BestMatch, &forecast, FETCHED_AT).unwrap();

        // Another city has no cache yet.
        let lyon = Location::new("Lyon", "", 45.75, 4.85);
        assert!(load(&dir, &lyon, Source::BestMatch, FETCHED_AT).is_none());

        // Another source has its own cache.
        assert!(load(&dir, &paris(), Source::MeteoFrance, FETCHED_AT).is_none());

        // Read back immediately: unchanged.
        let fresh = load(&dir, &paris(), Source::BestMatch, FETCHED_AT).unwrap();
        assert_eq!(fresh.forecast, forecast);
        assert_eq!(fresh.fetched_at, FETCHED_AT);

        // Two hours later: 14h and 15h are gone, 16h is "Now".
        let later = load(&dir, &paris(), Source::BestMatch, FETCHED_AT + 2 * HOUR).unwrap().forecast;
        assert_eq!(later.hours.len(), 2);
        assert_eq!(later.hours[0].label, "Now");
        assert_eq!(later.hours[1].label, "00h");
        assert_eq!(later.days[0].label, "Today");

        // Past midnight local time: Friday becomes "Today".
        let tomorrow = load(&dir, &paris(), Source::BestMatch, FETCHED_AT + 10 * HOUR).unwrap().forecast;
        assert_eq!(tomorrow.days.len(), 2);
        assert_eq!(tomorrow.days[0].label, "Today");
        assert_eq!(tomorrow.days[1].label, "Sat");
        assert_eq!((tomorrow.today_min, tomorrow.today_max), (6.5, 14.0));

        // Once every hour is over, the cache is not worth showing.
        assert!(load(&dir, &paris(), Source::BestMatch, FETCHED_AT + 11 * HOUR).is_none());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_or_corrupt_cache_is_none() {
        let dir = temp_dir("corrupt");
        assert!(load(&dir, &paris(), Source::BestMatch, FETCHED_AT).is_none());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(file_name(&paris(), Source::BestMatch)), b"{not json").unwrap();
        assert!(load(&dir, &paris(), Source::BestMatch, FETCHED_AT).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
