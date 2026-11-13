//! Data beyond the main forecast, each fetched separately so a failure never
//! blocks the forecast itself:
//!
//! - **Spread**: several weather models' hourly temperature and rain, to show
//!   how much they agree.
//! - **Air**: the European air quality index and pollen counts.
//! - **Normals**: the 1991–2020 average high and low for each day of the
//!   year, downloaded once per city and kept on disk.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Error, client};
use crate::model::{Forecast, Location};

/// Models compared for the agreement band (global and regional ones that
/// cover most places; those without data for a place are simply skipped).
pub const SPREAD_MODELS: [&str; 5] = ["ecmwf_ifs025", "icon_seamless", "gfs_seamless", "meteofrance_seamless", "ukmo_seamless"];

/// One hour across several models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    /// Local time, "2026-11-13T15:00".
    pub time: String,
    /// Coldest and warmest of the models, °C.
    pub t_min: f64,
    pub t_max: f64,
    /// How many models have data for this hour, and how many expect rain.
    pub models: u8,
    pub wet: u8,
}

/// Air quality and pollen now.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Air {
    /// European AQI (0 = clean; above 100 extremely poor).
    pub european_aqi: Option<f64>,
    pub pm2_5: Option<f64>,
    /// Grains/m³ per kind ("birch", "grass", …); empty where there is no
    /// pollen forecast (outside Europe).
    pub pollen: Vec<(String, f64)>,
}

/// The average high and low for each day of the year (index 0 = 1 January,
/// 365 entries; 29 February uses 28 February).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Normals {
    pub max: Vec<f64>,
    pub min: Vec<f64>,
}

// ── Spread ─────────────────────────────────────────────────────────────

pub async fn fetch_spread(location: &Location) -> Result<Vec<Spread>, Error> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}\
         &hourly=temperature_2m,precipitation&timezone=auto&forecast_days=7&models={}",
        location.latitude,
        location.longitude,
        SPREAD_MODELS.join(",")
    );
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_spread(&body)
}

/// Per hour, the range of the models' temperatures and how many expect rain
/// (≥ 0.1 mm). Hours where fewer than two models have data are left out.
pub fn parse_spread(json: &[u8]) -> Result<Vec<Spread>, Error> {
    let v: Value = serde_json::from_slice(json)?;
    let hourly = v.get("hourly").and_then(Value::as_object).ok_or(Error::MissingData)?;
    let times = hourly.get("time").and_then(Value::as_array).ok_or(Error::MissingData)?;
    let series = |prefix: &str| -> Vec<&Vec<Value>> {
        hourly.iter().filter(|(k, _)| k.starts_with(prefix)).filter_map(|(_, v)| v.as_array()).collect()
    };
    let (temps, rains) = (series("temperature_2m"), series("precipitation"));
    Ok(times
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let ts: Vec<f64> = temps.iter().filter_map(|s| s.get(i)?.as_f64()).collect();
            if ts.len() < 2 {
                return None;
            }
            let rs: Vec<f64> = rains.iter().filter_map(|s| s.get(i)?.as_f64()).collect();
            Some(Spread {
                time: t.as_str()?.to_string(),
                t_min: ts.iter().copied().fold(f64::INFINITY, f64::min),
                t_max: ts.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                models: ts.len() as u8,
                wet: rs.iter().filter(|&&r| r >= 0.1).count() as u8,
            })
        })
        .collect())
}

/// How much the models agree over some hours, 0–100: half from how close
/// their temperatures are, half from whether they agree on rain.
pub fn agreement(hours: &[&Spread]) -> Option<u8> {
    if hours.is_empty() {
        return None;
    }
    let n = hours.len() as f64;
    let mean_spread = hours.iter().map(|s| s.t_max - s.t_min).sum::<f64>() / n;
    let temp_score = 1.0 - (mean_spread / 8.0).clamp(0.0, 1.0);
    let rain_score = hours
        .iter()
        .map(|s| {
            let m = f64::from(s.models.max(1));
            f64::from(s.wet).max(m - f64::from(s.wet)) / m
        })
        .sum::<f64>()
        / n;
    // Rain agreement is at least 0.5 by construction; stretch it to 0–1.
    let rain_score = ((rain_score - 0.5) * 2.0).clamp(0.0, 1.0);
    Some(((temp_score * 0.5 + rain_score * 0.5) * 100.0).round() as u8)
}

// ── Air ────────────────────────────────────────────────────────────────

const POLLEN: [&str; 6] = ["alder", "birch", "grass", "mugwort", "olive", "ragweed"];

pub async fn fetch_air(location: &Location) -> Result<Air, Error> {
    let pollen: Vec<String> = POLLEN.iter().map(|p| format!("{p}_pollen")).collect();
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}\
         &current=european_aqi,pm2_5,{}&timezone=auto",
        location.latitude,
        location.longitude,
        pollen.join(",")
    );
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_air(&body)
}

pub fn parse_air(json: &[u8]) -> Result<Air, Error> {
    let v: Value = serde_json::from_slice(json)?;
    let c = v.get("current").ok_or(Error::MissingData)?;
    let num = |k: &str| c.get(k).and_then(Value::as_f64);
    Ok(Air {
        european_aqi: num("european_aqi"),
        pm2_5: num("pm2_5"),
        pollen: POLLEN.iter().filter_map(|p| Some((p.to_string(), num(&format!("{p}_pollen"))?))).collect(),
    })
}

// ── Normals ────────────────────────────────────────────────────────────

/// Downloads 30 years of daily highs and lows (about 250 KB) and averages
/// them per day of the year, smoothed over ±7 days.
pub async fn fetch_normals(location: &Location) -> Result<Normals, Error> {
    let url = format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={}&longitude={}\
         &start_date=1991-01-01&end_date=2020-12-31&daily=temperature_2m_max,temperature_2m_min&timezone=auto",
        location.latitude, location.longitude
    );
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_normals(&body)
}

pub fn parse_normals(json: &[u8]) -> Result<Normals, Error> {
    #[derive(Deserialize)]
    struct Raw {
        daily: RawDaily,
    }
    #[derive(Deserialize)]
    struct RawDaily {
        time: Vec<String>,
        temperature_2m_max: Vec<Option<f64>>,
        temperature_2m_min: Vec<Option<f64>>,
    }
    let raw: Raw = serde_json::from_slice(json)?;
    let (mut sum_max, mut sum_min, mut count) = (vec![0.0; 365], vec![0.0; 365], vec![0.0; 365]);
    for (i, date) in raw.daily.time.iter().enumerate() {
        let (Some(day), Some(hi), Some(lo)) = (
            day_of_year(date),
            raw.daily.temperature_2m_max.get(i).copied().flatten(),
            raw.daily.temperature_2m_min.get(i).copied().flatten(),
        ) else {
            continue;
        };
        sum_max[day] += hi;
        sum_min[day] += lo;
        count[day] += 1.0;
    }
    if count.iter().all(|&c| c == 0.0) {
        return Err(Error::MissingData);
    }
    let smooth = |sum: &[f64]| -> Vec<f64> {
        (0..365)
            .map(|d| {
                let (s, n) = (-7i32..=7).fold((0.0, 0.0), |(s, n), k| {
                    let j = (d as i32 + k).rem_euclid(365) as usize;
                    (s + sum[j], n + count[j])
                });
                if n > 0.0 { s / n } else { 0.0 }
            })
            .collect()
    };
    Ok(Normals { max: smooth(&sum_max), min: smooth(&sum_min) })
}

/// 0-based day of the year for "YYYY-MM-DD"; 29 February counts as 28.
pub fn day_of_year(date: &str) -> Option<usize> {
    const START: [usize; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let m: usize = date.get(5..7)?.parse().ok()?;
    let d: usize = date.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some((START[m - 1] + d.min(if m == 2 { 28 } else { 31 }) - 1).min(364))
}

fn normals_file(location: &Location) -> String {
    format!("normals-{}.json", location.key())
}

pub fn load_normals(dir: &Path, location: &Location) -> Option<Normals> {
    serde_json::from_slice(&std::fs::read(dir.join(normals_file(location))).ok()?).ok()
}

pub fn save_normals(dir: &Path, location: &Location, normals: &Normals) -> io::Result<()> {
    crate::cache::write_atomic(dir, &normals_file(location), &serde_json::to_vec(normals)?)
}

/// The city's normals from disk, or downloaded and saved the first time.
pub async fn normals_for(dir: &Path, location: &Location) -> Option<Normals> {
    if let Some(n) = load_normals(dir, location) {
        return Some(n);
    }
    let n = fetch_normals(location).await.ok()?;
    let _ = save_normals(dir, location, &n);
    Some(n)
}

// ── Putting it together ────────────────────────────────────────────────

/// Everything for one place: the forecast (required) with the models'
/// spread and the air quality fetched alongside, and the normals (from disk
/// in `dir`, or downloaded once). Only the forecast's own failure is an error.
pub async fn download(dir: Option<&Path>, location: &Location, source: crate::Source) -> Result<Forecast, Error> {
    let (forecast, spread, air) = tokio::join!(
        crate::api::fetch_forecast(location, source),
        fetch_spread(location),
        fetch_air(location),
    );
    let mut forecast = forecast?;
    let normals = match dir {
        Some(dir) => normals_for(dir, location).await,
        None => None,
    };
    attach(&mut forecast, spread.ok(), air.ok(), normals.as_ref());
    Ok(forecast)
}

/// Adds the extras to a forecast: spread by hour, air, and each day's normal.
pub fn attach(forecast: &mut Forecast, spread: Option<Vec<Spread>>, air: Option<Air>, normals: Option<&Normals>) {
    if let Some(spread) = spread {
        forecast.spread = spread;
    }
    forecast.air = air;
    if let Some(n) = normals {
        for day in &mut forecast.days {
            if let Some(i) = day_of_year(&day.date) {
                day.normal_max = n.max.get(i).copied();
                day.normal_min = n.min.get(i).copied();
            }
        }
    }
}

/// Spread entries by time, for looking up the hours of a day.
pub fn spread_by_time(spread: &[Spread]) -> HashMap<&str, &Spread> {
    spread.iter().map(|s| (s.time.as_str(), s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_from_several_models() {
        let json = br#"{"hourly": {"time": ["2026-11-13T10:00", "2026-11-13T11:00"],
            "temperature_2m_a": [10.0, 11.0], "precipitation_a": [0.0, 1.0],
            "temperature_2m_b": [12.0, null], "precipitation_b": [0.5, null],
            "temperature_2m_c": [11.0, 11.5], "precipitation_c": [0.0, 0.8]}}"#;
        let s = parse_spread(json).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].t_min, s[0].t_max, s[0].models, s[0].wet), (10.0, 12.0, 3, 1));
        assert_eq!((s[1].models, s[1].wet), (2, 2));
        // Perfect agreement → 100; split on rain and far apart → low.
        let same = Spread { time: String::new(), t_min: 10.0, t_max: 10.0, models: 4, wet: 4 };
        let split = Spread { time: String::new(), t_min: 5.0, t_max: 13.0, models: 4, wet: 2 };
        assert_eq!(agreement(&[&same]), Some(100));
        assert_eq!(agreement(&[&split]), Some(0));
    }

    #[test]
    fn air_and_pollen() {
        let json = br#"{"current": {"european_aqi": 28, "pm2_5": 8.8, "birch_pollen": 12.0, "grass_pollen": null}}"#;
        let a = parse_air(json).unwrap();
        assert_eq!(a.european_aqi, Some(28.0));
        assert_eq!(a.pollen, vec![("birch".to_string(), 12.0)]);
    }

    #[test]
    fn normals_by_day_of_year() {
        assert_eq!(day_of_year("2026-01-01"), Some(0));
        assert_eq!(day_of_year("2024-02-29"), Some(58));
        assert_eq!(day_of_year("2026-12-31"), Some(364));
        let json = br#"{"daily": {"time": ["1991-11-13", "1992-11-13"],
            "temperature_2m_max": [10.0, 12.0], "temperature_2m_min": [2.0, 4.0]}}"#;
        let n = parse_normals(json).unwrap();
        let d = day_of_year("2026-11-13").unwrap();
        assert_eq!(n.max[d], 11.0);
        assert_eq!(n.min[d], 3.0);
        // Smoothed: a nearby day borrows the same values.
        assert_eq!(n.max[d + 3], 11.0);
    }
}
