use std::sync::OnceLock;

use serde::Deserialize;

use crate::condition::Condition;
use crate::date::weekday_short;
use crate::model::{Current, Day, Forecast, Hour, Location};
use crate::source::Source;

/// Hours kept from each response. Screens show 24; the rest lets a cached
/// forecast still fill the chart a day later.
const HOURS_KEPT: usize = 48;
const DAYS_SHOWN: usize = 7;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("unexpected response: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("response has no data for the current hour")]
    MissingData,
    #[error("couldn't save the forecast: {0}")]
    Cache(#[from] std::io::Error),
}

/// Fetches the forecast for `location` from Open-Meteo (no API key needed),
/// using the weather model of `source`.
pub async fn fetch_forecast(location: &Location, source: Source) -> Result<Forecast, Error> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast\
         ?latitude={}&longitude={}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,is_day,weather_code,wind_speed_10m,wind_direction_10m\
         &hourly=temperature_2m,precipitation,precipitation_probability,weather_code,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_sum,precipitation_probability_max,sunrise,sunset\
         &timezone=auto&forecast_days={}&models={}",
        location.latitude,
        location.longitude,
        DAYS_SHOWN,
        source.model_id()
    );
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_forecast(&location.name, &body)
}

/// One shared HTTP client (connection pool, TLS setup done once).
pub(crate) fn client() -> Result<&'static reqwest::Client, Error> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(c) = CLIENT.get() {
        return Ok(c);
    }
    let builder = reqwest::Client::builder();
    // On Android the platform verifier needs JNI initialisation from the
    // Java side; bundled Mozilla roots avoid that and work in any host app
    // (Tauri, Slint, a widget). Elsewhere the OS certificate store is used.
    #[cfg(target_os = "android")]
    let builder = builder.tls_certs_only(
        webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|der| reqwest::Certificate::from_der(der).ok()),
    );
    Ok(CLIENT.get_or_init(|| builder.build().expect("valid HTTP client config")))
}

/// Turns a raw Open-Meteo JSON response into a [`Forecast`].
pub fn parse_forecast(location_name: &str, json: &[u8]) -> Result<Forecast, Error> {
    let raw: RawResponse = serde_json::from_slice(json)?;
    let c = raw.current;

    // Hourly times look like "2026-11-12T14:00"; the first 13 chars identify the hour.
    let current_hour = c.time.get(..13).ok_or(Error::MissingData)?;
    let start = raw
        .hourly
        .time
        .iter()
        .position(|t| t.get(..13).is_some_and(|h| h >= current_hour))
        .ok_or(Error::MissingData)?;

    // Some models stop before 7 days: their values turn to null. Hours and
    // days end at the first gap.
    let h = &raw.hourly;
    let hour_ok = |i: usize| at(&h.temperature_2m, i).is_some() && at(&h.weather_code, i).is_some();
    let hour = |i: usize| Hour {
        label: format!("{}h", h.time[i].get(11..13).unwrap_or("?")),
        time: h.time[i].clone(),
        temperature: at(&h.temperature_2m, i).unwrap_or_default(),
        precipitation_probability: at(&h.precipitation_probability, i).unwrap_or(0.0),
        precipitation: at(&h.precipitation, i).unwrap_or(0.0),
        condition: Condition::from_wmo(at(&h.weather_code, i).unwrap_or(0)),
        is_day: at(&h.is_day, i) == Some(1),
    };
    let mut hours: Vec<Hour> = (start..h.time.len())
        .take_while(|&i| hour_ok(i))
        .take(HOURS_KEPT)
        .map(hour)
        .collect();
    if let Some(first) = hours.first_mut() {
        first.label = "Now".into();
    }
    if hours.is_empty() {
        return Err(Error::MissingData);
    }

    let d = &raw.daily;
    let days: Vec<Day> = (0..d.time.len().min(DAYS_SHOWN))
        .map_while(|i| {
            let (min, max) = (at(&d.temperature_2m_min, i)?, at(&d.temperature_2m_max, i)?);
            let date = d.time[i].clone();
            // Every hour of this day (local time), for the day's detail chart.
            let day_hours: Vec<Hour> = (0..h.time.len())
                .filter(|&j| h.time[j].starts_with(&date) && hour_ok(j))
                .map(hour)
                .collect();
            Some(Day {
                label: if i == 0 {
                    "Today".into()
                } else {
                    weekday_short(&date).unwrap_or("?").into()
                },
                min,
                max,
                precipitation_probability: at(&d.precipitation_probability_max, i).unwrap_or(0.0),
                precipitation_sum: at(&d.precipitation_sum, i)
                    .unwrap_or_else(|| day_hours.iter().map(|h| h.precipitation).sum()),
                condition: Condition::from_wmo(at(&d.weather_code, i).unwrap_or(0)),
                hours: day_hours,
                sunrise: d.sunrise.get(i).cloned().flatten(),
                sunset: d.sunset.get(i).cloned().flatten(),
                date,
            })
        })
        .collect();
    let today = days.first().ok_or(Error::MissingData)?;

    Ok(Forecast {
        location: location_name.to_owned(),
        utc_offset_seconds: raw.utc_offset_seconds,
        current: Current {
            condition: Condition::from_wmo(c.weather_code),
            is_day: c.is_day == 1,
            time: c.time,
            temperature: c.temperature_2m,
            apparent_temperature: c.apparent_temperature,
            humidity: c.relative_humidity_2m,
            wind_speed: c.wind_speed_10m,
            wind_direction: c.wind_direction_10m,
        },
        today_min: today.min,
        today_max: today.max,
        hours,
        days,
    })
}

#[derive(Deserialize)]
struct RawResponse {
    #[serde(default)]
    utc_offset_seconds: i64,
    current: RawCurrent,
    hourly: RawHourly,
    daily: RawDaily,
}

#[derive(Deserialize)]
struct RawCurrent {
    time: String,
    temperature_2m: f64,
    apparent_temperature: f64,
    relative_humidity_2m: f64,
    is_day: u8,
    weather_code: u8,
    wind_speed_10m: f64,
    #[serde(default)]
    wind_direction_10m: f64,
}

/// The value at `i`, if present and not null.
fn at<T: Copy>(values: &[Option<T>], i: usize) -> Option<T> {
    values.get(i).copied().flatten()
}

#[derive(Deserialize)]
struct RawHourly {
    time: Vec<String>,
    #[serde(default)]
    temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    precipitation: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability: Vec<Option<f64>>,
    #[serde(default)]
    weather_code: Vec<Option<u8>>,
    #[serde(default)]
    is_day: Vec<Option<u8>>,
}

#[derive(Deserialize)]
struct RawDaily {
    time: Vec<String>,
    #[serde(default)]
    sunrise: Vec<Option<String>>,
    #[serde(default)]
    sunset: Vec<Option<String>>,
    #[serde(default)]
    weather_code: Vec<Option<u8>>,
    #[serde(default)]
    precipitation_sum: Vec<Option<f64>>,
    #[serde(default)]
    precipitation_probability_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_max: Vec<Option<f64>>,
    #[serde(default)]
    temperature_2m_min: Vec<Option<f64>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "current": {"time": "2026-11-12T14:15", "temperature_2m": 11.2, "apparent_temperature": 9.1,
                    "relative_humidity_2m": 78, "is_day": 1, "weather_code": 61, "wind_speed_10m": 14.4},
        "hourly": {
            "time": ["2026-11-12T13:00", "2026-11-12T14:00", "2026-11-12T15:00"],
            "temperature_2m": [10.8, 11.2, 11.5],
            "precipitation_probability": [40, null, 70],
            "weather_code": [3, 61, 63],
            "is_day": [1, 1, 1]
        },
        "daily": {"time": ["2026-11-12", "2026-11-13"], "weather_code": [61, 0],
                  "temperature_2m_max": [12.4, 14.0], "temperature_2m_min": [7.9, 6.5],
                  "precipitation_probability_max": [80, null]}
    }"#;

    #[test]
    fn parses_sample_and_starts_at_current_hour() {
        let f = parse_forecast("Paris", SAMPLE.as_bytes()).unwrap();
        assert_eq!(f.current.condition, Condition::Rain);
        assert_eq!(f.hours.len(), 2);
        assert_eq!(f.hours[0].label, "Now");
        assert_eq!(f.hours[0].precipitation_probability, 0.0);
        assert_eq!(f.hours[1].label, "15h");
        assert_eq!(f.today_max, 12.4);
        assert_eq!(f.days.len(), 2);
        assert_eq!(f.days[0].label, "Today");
        assert_eq!(f.days[1].label, "Fri"); // 2026-11-13
        assert_eq!(f.days[1].condition, Condition::Clear);
        assert_eq!(f.days[1].precipitation_probability, 0.0);
    }

    #[test]
    fn stops_where_the_model_has_no_data_and_keeps_rain() {
        let json = r#"{
            "current": {"time": "2026-11-12T22:15", "temperature_2m": 8.0, "apparent_temperature": 6.0,
                        "relative_humidity_2m": 90, "is_day": 0, "weather_code": 61, "wind_speed_10m": 10},
            "hourly": {
                "time": ["2026-11-12T22:00", "2026-11-12T23:00", "2026-11-13T00:00", "2026-11-13T01:00"],
                "temperature_2m": [8.0, 7.5, 7.0, null],
                "precipitation": [0.4, 1.2, null, null],
                "precipitation_probability": [null, null, null, null],
                "weather_code": [61, 63, 3, null],
                "is_day": [0, 0, 0, null]
            },
            "daily": {"time": ["2026-11-12", "2026-11-13", "2026-11-14"], "weather_code": [61, 3, null],
                      "temperature_2m_max": [12.0, 10.0, null], "temperature_2m_min": [6.0, 5.0, null],
                      "precipitation_sum": [3.1, null, null], "precipitation_probability_max": [null, null, null]}
        }"#;
        let f = parse_forecast("Paris", json.as_bytes()).unwrap();
        assert_eq!(f.hours.len(), 3, "ends at the first missing hour");
        assert_eq!(f.hours[1].precipitation, 1.2);
        assert_eq!(f.hours[0].precipitation_probability, 0.0);
        assert_eq!(f.days.len(), 2, "ends at the first missing day");
        let rain = |d: &Day| d.hours.iter().map(|h| h.precipitation).collect::<Vec<_>>();
        assert_eq!(rain(&f.days[0]), vec![0.4, 1.2]);
        assert_eq!(f.days[0].hours[1].label, "23h");
        assert_eq!(f.days[0].precipitation_sum, 3.1);
        // No daily total from the model: the sum of the hours.
        assert_eq!(rain(&f.days[1]), vec![0.0]);
        assert_eq!(f.days[1].precipitation_sum, 0.0);
    }

    #[tokio::test]
    #[ignore = "hits the network"]
    async fn fetches_live_forecast() {
        let f = fetch_forecast(&Location::paris(), Source::BestMatch).await.unwrap();
        assert_eq!(f.hours.len(), HOURS_KEPT);
        assert_eq!(f.days.len(), DAYS_SHOWN);
        // A shorter-range model still parses, with fewer days.
        let mf = fetch_forecast(&Location::paris(), Source::MeteoFrance).await.unwrap();
        assert!((3..=DAYS_SHOWN).contains(&mf.days.len()));
    }
}
