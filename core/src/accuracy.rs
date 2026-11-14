//! Which data source is most accurate where you live: each model's forecast
//! made the day before, compared with what happened (the ERA5 reanalysis,
//! which arrives about five days late) over the last two weeks or so.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Error, client};
use crate::model::Location;
use crate::source::Source;

/// Days of forecasts asked for; ERA5 covers all but the last five or so.
const PAST_DAYS: u32 = 21;
/// A day with at least this much rain counts as a rainy day.
const WET_DAY_MM: f64 = 1.0;
/// Fewer hours than this compared and the score means nothing.
const MIN_HOURS: usize = 72;
/// Scores are worked out again after this long.
pub const MAX_AGE_SECS: i64 = 3 * 24 * 3600;

/// How one source did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub source: Source,
    /// Average miss of the hourly temperature, °C.
    pub temp_error: f64,
    /// Days whose "rainy or dry" the forecast got right, out of `days`.
    pub rain_right: u8,
    pub days: u8,
}

impl Score {
    /// Lower is better: a degree of temperature miss weighs as much as
    /// getting a quarter of the days' rain wrong.
    pub fn penalty(&self) -> f64 {
        let wrong = f64::from(self.days - self.rain_right) / f64::from(self.days.max(1));
        self.temp_error + 4.0 * wrong
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Accuracy {
    /// Best first.
    pub scores: Vec<Score>,
    /// The dates compared, "2026-10-24" style.
    pub from: String,
    pub to: String,
    pub computed_at: i64,
}

impl Accuracy {
    pub fn score(&self, source: Source) -> Option<&Score> {
        self.scores.iter().find(|s| s.source == source)
    }

    pub fn best(&self) -> Option<Source> {
        self.scores.first().map(|s| s.source)
    }
}

pub async fn fetch(location: &Location, now: i64) -> Result<Accuracy, Error> {
    let models: Vec<&str> = Source::ALL.iter().map(|s| s.model_id()).collect();
    let forecasts = format!(
        "https://previous-runs-api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &hourly=temperature_2m_previous_day1,precipitation_previous_day1\
         &past_days={PAST_DAYS}&forecast_days=1&timezone=auto&models={}",
        location.latitude,
        location.longitude,
        models.join(",")
    );
    let forecasts = client()?.get(forecasts).send().await?.error_for_status()?.bytes().await?;
    let times = serde_json::from_slice::<Value>(&forecasts)?;
    let times = times["hourly"]["time"].as_array().ok_or(Error::MissingData)?;
    let date = |v: Option<&Value>| v.and_then(Value::as_str).and_then(|t| t.get(..10)).map(str::to_owned);
    let (start, end) = (date(times.first()).ok_or(Error::MissingData)?, date(times.last()).ok_or(Error::MissingData)?);
    let truth = format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={}&longitude={}\
         &hourly=temperature_2m,precipitation&start_date={start}&end_date={end}\
         &models=era5_seamless&timezone=auto",
        location.latitude, location.longitude
    );
    let truth = client()?.get(truth).send().await?.error_for_status()?.bytes().await?;
    compare(&forecasts, &truth, now)
}

/// The scores, from the previous-runs answer and the reanalysis answer.
pub fn compare(forecasts: &[u8], truth: &[u8], now: i64) -> Result<Accuracy, Error> {
    let f: Value = serde_json::from_slice(forecasts)?;
    let t: Value = serde_json::from_slice(truth)?;
    let numbers = |v: &Value| -> Vec<Option<f64>> {
        v.as_array().map(|a| a.iter().map(Value::as_f64).collect()).unwrap_or_default()
    };
    let strings = |v: &Value| -> Vec<String> {
        v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default()
    };
    let truth_time = strings(&t["hourly"]["time"]);
    let truth_temp = numbers(&t["hourly"]["temperature_2m"]);
    let truth_rain = numbers(&t["hourly"]["precipitation"]);
    let time = strings(&f["hourly"]["time"]);
    let index: std::collections::HashMap<&str, usize> =
        truth_time.iter().enumerate().map(|(i, s)| (s.as_str(), i)).collect();

    let mut scores = Vec::new();
    let mut span: Option<(String, String)> = None;
    for source in Source::ALL {
        let id = source.model_id();
        let temp = numbers(&f["hourly"][format!("temperature_2m_previous_day1_{id}")]);
        let rain = numbers(&f["hourly"][format!("precipitation_previous_day1_{id}")]);
        let (mut miss, mut hours) = (0.0, 0usize);
        // Per day: (forecast mm, observed mm, hours with both).
        let mut days: Vec<(String, f64, f64, usize)> = Vec::new();
        for (i, at) in time.iter().enumerate() {
            let Some(&j) = index.get(at.as_str()) else { continue };
            if let (Some(Some(a)), Some(Some(b))) = (temp.get(i), truth_temp.get(j)) {
                miss += (a - b).abs();
                hours += 1;
            }
            if let (Some(Some(a)), Some(Some(b))) = (rain.get(i), truth_rain.get(j)) {
                let day = &at[..10.min(at.len())];
                if days.last().is_none_or(|d| d.0 != day) {
                    days.push((day.to_owned(), 0.0, 0.0, 0));
                }
                let d = days.last_mut().expect("just pushed");
                (d.1, d.2, d.3) = (d.1 + a, d.2 + b, d.3 + 1);
            }
        }
        days.retain(|d| d.3 == 24); // whole days only
        if hours < MIN_HOURS || days.is_empty() {
            continue;
        }
        if span.is_none() {
            span = Some((days[0].0.clone(), days[days.len() - 1].0.clone()));
        }
        let right = days.iter().filter(|d| (d.1 >= WET_DAY_MM) == (d.2 >= WET_DAY_MM)).count();
        scores.push(Score {
            source,
            temp_error: (miss / hours as f64 * 10.0).round() / 10.0,
            rain_right: right as u8,
            days: days.len() as u8,
        });
    }
    let (from, to) = span.ok_or(Error::MissingData)?;
    scores.sort_by(|a, b| a.penalty().total_cmp(&b.penalty()));
    Ok(Accuracy { scores, from, to, computed_at: now })
}

fn file(location: &Location) -> String {
    format!("accuracy-{}.json", location.key())
}

pub fn load(dir: &Path, location: &Location) -> Option<Accuracy> {
    serde_json::from_slice(&std::fs::read(dir.join(file(location))).ok()?).ok()
}

pub fn save(dir: &Path, location: &Location, accuracy: &Accuracy) -> io::Result<()> {
    crate::cache::write_atomic(dir, &file(location), &serde_json::to_vec(accuracy)?)
}

/// The saved scores if they're recent, otherwise worked out again and saved.
pub async fn for_place(dir: Option<&Path>, location: &Location, now: i64) -> Option<Accuracy> {
    let saved = dir.and_then(|d| load(d, location));
    if let Some(a) = &saved
        && now - a.computed_at < MAX_AGE_SECS
    {
        return saved;
    }
    match fetch(location, now).await {
        Ok(a) => {
            if let Some(d) = dir {
                let _ = save(d, location, &a);
            }
            Some(a)
        }
        Err(_) => saved, // old scores beat none
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 48 hours: two days. ECMWF is 1° off with the rain right; ICON is
    /// 3° off and misses the rainy day.
    fn answers() -> (String, String) {
        let time: Vec<String> = (0..48).map(|h| format!("\"2026-11-0{}T{:02}:00\"", 1 + h / 24, h % 24)).collect();
        let truth_temp: Vec<String> = (0..48).map(|h| format!("{}", 5 + h % 3)).collect();
        let truth_rain: Vec<&str> = (0..48).map(|h| if h == 30 { "4" } else { "0" }).collect();
        let shifted = |by: i32| -> Vec<String> { (0..48).map(|h| format!("{}", 5 + h % 3 + by)).collect() };
        let truth = format!(
            r#"{{"hourly":{{"time":[{}],"temperature_2m":[{}],"precipitation":[{}]}}}}"#,
            time.join(","),
            truth_temp.join(","),
            truth_rain.join(",")
        );
        let zeros = vec!["0"; 48].join(",");
        let forecasts = format!(
            r#"{{"hourly":{{"time":[{}],
            "temperature_2m_previous_day1_ecmwf_ifs025":[{}],"precipitation_previous_day1_ecmwf_ifs025":[{}],
            "temperature_2m_previous_day1_icon_seamless":[{}],"precipitation_previous_day1_icon_seamless":[{}]}}}}"#,
            time.join(","),
            shifted(-1).join(","),
            truth_rain.join(","),
            shifted(3).join(","),
            zeros
        );
        (forecasts, truth)
    }

    #[test]
    fn too_few_hours_say_nothing() {
        let (f, t) = answers(); // 48 hours < 72
        assert!(compare(f.as_bytes(), t.as_bytes(), 0).is_err());
    }

    #[test]
    fn scores_over_four_days() {
        // The same two days twice over, under other dates: 96 hours.
        let (f, t) = answers();
        let double = |s: &str| -> String {
            let v: Value = serde_json::from_str(s).unwrap();
            let mut out = serde_json::Map::new();
            for (k, arr) in v["hourly"].as_object().unwrap() {
                let mut a = arr.as_array().unwrap().clone();
                let more: Vec<Value> = a
                    .iter()
                    .map(|x| match x.as_str() {
                        Some(s) => Value::from(s.replace("2026-11-0", "2026-11-1")),
                        None => x.clone(),
                    })
                    .collect();
                a.extend(more);
                out.insert(k.clone(), Value::Array(a));
            }
            serde_json::json!({ "hourly": out }).to_string()
        };
        let a = compare(double(&f).as_bytes(), double(&t).as_bytes(), 7).unwrap();
        assert_eq!(a.best(), Some(Source::Ecmwf));
        let ecmwf = a.score(Source::Ecmwf).unwrap();
        assert_eq!((ecmwf.temp_error, ecmwf.rain_right, ecmwf.days), (1.0, 4, 4));
        let icon = a.score(Source::Dwd).unwrap();
        assert_eq!((icon.temp_error, icon.rain_right, icon.days), (3.0, 2, 4));
        assert_eq!((a.from.as_str(), a.to.as_str(), a.computed_at), ("2026-11-01", "2026-11-12", 7));
        assert!(a.score(Source::Noaa).is_none(), "no data, no score");
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_paris() {
        let place = Location::paris();
        let a = fetch(&place, 0).await.unwrap();
        for s in &a.scores {
            println!("{:?}: ±{}° rain {}/{}", s.source, s.temp_error, s.rain_right, s.days);
        }
        println!("{} → {}", a.from, a.to);
    }
}
