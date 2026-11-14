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

/// One hour across the ensemble's runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    /// Local time, "2026-11-13T15:00".
    pub time: String,
    /// Low end, middle and high end of the runs' temperatures, °C.
    pub t_min: f64,
    #[serde(default)]
    pub t_mid: f64,
    pub t_max: f64,
    /// How many runs have data for this hour, and how many expect rain.
    pub models: u8,
    pub wet: u8,
    /// Low and high end of the rain the runs expect this hour, mm.
    #[serde(default)]
    pub r_min: f64,
    #[serde(default)]
    pub r_max: f64,
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
    /// Each day's peak per kind, today first (4 days; Europe only).
    #[serde(default)]
    pub pollen_days: Vec<PollenDay>,
}

/// One day's pollen: the highest count of each kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PollenDay {
    /// "2026-11-13".
    pub date: String,
    pub pollen: Vec<(String, f64)>,
}

/// The average high and low for each day of the year (index 0 = 1 January,
/// 365 entries; 29 February uses 28 February).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Normals {
    pub max: Vec<f64>,
    pub min: Vec<f64>,
    /// Each date's highest high and lowest low since 1991, with the year
    /// (not smoothed). Empty in files saved before records existed.
    #[serde(default)]
    pub record_max: Vec<(f64, u16)>,
    #[serde(default)]
    pub record_min: Vec<(f64, u16)>,
}

// ── Ensemble: one model, 51 runs ───────────────────────────────────

/// The ECMWF ensemble: one model run 51 times from slightly different
/// starting points. How far the runs spread is how sure the forecast is; the
/// share of runs with rain is an honest chance of rain.
pub const ENSEMBLE_MODEL: &str = "ecmwf_ifs025";
/// Rain in an hour, and over a day (the usual "rainy day"), in mm.
const ENSEMBLE_WET_HOUR_MM: f64 = 0.1;
const ENSEMBLE_WET_DAY_MM: f64 = 1.0;
/// The ranges shown keep the middle 80 % of the runs (a single odd run
/// doesn't widen them).
const LOW: f64 = 0.1;
const HIGH: f64 = 0.9;

/// What the runs say for one day.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnsembleDay {
    /// The day's high: low end, middle and high end of the runs, °C.
    pub high: (f64, f64, f64),
    /// Runs with at least 1 mm of rain, out of `runs`.
    pub wet: u8,
    pub runs: u8,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ensemble {
    pub runs: u8,
    /// Local hour ("2026-11-14T15:00") → share of runs with rain, 0–1.
    pub hours: HashMap<String, f64>,
    /// Local date → the day's high and wet runs.
    pub days: HashMap<String, EnsembleDay>,
    /// Per hour, the range of the runs' temperatures and rain.
    pub spread: Vec<Spread>,
}

pub async fn fetch_ensemble(location: &Location) -> Result<Ensemble, Error> {
    let url = format!(
        "https://ensemble-api.open-meteo.com/v1/ensemble?latitude={}&longitude={}\
         &hourly=temperature_2m,precipitation&models={ENSEMBLE_MODEL}&forecast_days=7&timezone=auto",
        location.latitude, location.longitude
    );
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_ensemble(&body)
}

/// The value below which `share` of `sorted` lies, between neighbours.
fn quantile(sorted: &[f64], share: f64) -> f64 {
    let at = share * (sorted.len() - 1) as f64;
    let (i, frac) = (at.floor() as usize, at.fract());
    let next = sorted.get(i + 1).copied().unwrap_or(sorted[i]);
    sorted[i] + (next - sorted[i]) * frac
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v
}

pub fn parse_ensemble(json: &[u8]) -> Result<Ensemble, Error> {
    let v: Value = serde_json::from_slice(json)?;
    let hourly = v.get("hourly").and_then(Value::as_object).ok_or(Error::MissingData)?;
    let times = hourly.get("time").and_then(Value::as_array).ok_or(Error::MissingData)?;
    // "precipitation" is the control run, "precipitation_memberNN" the others.
    let series = |prefix: &str| -> Vec<&Vec<Value>> {
        hourly.iter().filter(|(k, _)| k.starts_with(prefix)).filter_map(|(_, v)| v.as_array()).collect()
    };
    let (rains, temps) = (series("precipitation"), series("temperature_2m"));
    if rains.len() < 2 {
        return Err(Error::MissingData);
    }
    let mut hours = HashMap::new();
    let mut spread = Vec::new();
    // Per date, each run's rain total and high.
    let mut totals: HashMap<String, Vec<f64>> = HashMap::new();
    let mut highs: HashMap<String, Vec<f64>> = HashMap::new();
    for (i, t) in times.iter().enumerate() {
        let Some(t) = t.as_str() else { continue };
        let at = |runs: &[&Vec<Value>]| -> Vec<Option<f64>> {
            runs.iter().map(|r| r.get(i).and_then(Value::as_f64)).collect()
        };
        let (rain, temp) = (at(&rains), at(&temps));
        let known: Vec<f64> = rain.iter().flatten().copied().collect();
        if known.len() < rains.len() / 2 {
            continue;
        }
        let wet = known.iter().filter(|&&mm| mm >= ENSEMBLE_WET_HOUR_MM).count();
        hours.insert(t.to_string(), wet as f64 / known.len() as f64);
        let date = t.get(..10).unwrap_or("").to_string();
        let day = totals.entry(date.clone()).or_insert_with(|| vec![0.0; rains.len()]);
        for (run, mm) in rain.iter().enumerate() {
            day[run] += mm.unwrap_or(0.0);
        }
        let ts: Vec<f64> = temp.iter().flatten().copied().collect();
        if ts.len() * 2 >= temps.len() && !ts.is_empty() {
            let high = highs.entry(date).or_insert_with(|| vec![f64::NEG_INFINITY; temps.len()]);
            for (run, t) in temp.iter().enumerate() {
                if let Some(t) = t {
                    high[run] = high[run].max(*t);
                }
            }
            let (ts, rs) = (sorted(ts), sorted(known));
            spread.push(Spread {
                time: t.to_string(),
                t_min: quantile(&ts, LOW),
                t_mid: quantile(&ts, 0.5),
                t_max: quantile(&ts, HIGH),
                models: rs.len().min(255) as u8,
                wet: wet.min(255) as u8,
                r_min: quantile(&rs, LOW).max(0.0),
                r_max: quantile(&rs, HIGH),
            });
        }
    }
    let runs = rains.len().min(255) as u8;
    let days = totals
        .into_iter()
        .filter_map(|(date, rain)| {
            let high = sorted(highs.get(&date)?.iter().copied().filter(|t| t.is_finite()).collect());
            if high.is_empty() {
                return None;
            }
            let wet = rain.iter().filter(|&&mm| mm >= ENSEMBLE_WET_DAY_MM).count().min(255) as u8;
            let high = (quantile(&high, LOW), quantile(&high, 0.5), quantile(&high, HIGH));
            Some((date, EnsembleDay { high, wet, runs }))
        })
        .collect();
    Ok(Ensemble { runs, hours, days, spread })
}

/// The ensemble's chances of rain replace the single model's; its spread
/// and each day's high and wet runs come along.
pub fn attach_ensemble(forecast: &mut Forecast, ensemble: Ensemble) {
    let chance = |share: f64| (share * 100.0).round();
    for h in forecast.hours.iter_mut().chain(forecast.days.iter_mut().flat_map(|d| d.hours.iter_mut())) {
        if let Some(&share) = ensemble.hours.get(&h.time) {
            h.precipitation_probability = chance(share);
        }
    }
    for d in &mut forecast.days {
        if let Some(e) = ensemble.days.get(&d.date) {
            d.precipitation_probability = chance(f64::from(e.wet) / f64::from(e.runs.max(1)));
            d.ensemble = Some(*e);
        }
    }
    forecast.ensemble_runs = ensemble.runs;
    forecast.spread = ensemble.spread;
}

// ── Air ────────────────────────────────────────────────────────────────

const POLLEN: [&str; 6] = ["alder", "birch", "grass", "mugwort", "olive", "ragweed"];

pub async fn fetch_air(location: &Location) -> Result<Air, Error> {
    let pollen: Vec<String> = POLLEN.iter().map(|p| format!("{p}_pollen")).collect();
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}\
         &current=european_aqi,pm2_5,{pollen}&hourly={pollen}&forecast_days=4&timezone=auto",
        location.latitude,
        location.longitude,
        pollen = pollen.join(",")
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
        pollen_days: v.get("hourly").map(pollen_days).unwrap_or_default(),
    })
}

/// Hourly pollen grouped into days, keeping each kind's peak.
fn pollen_days(hourly: &Value) -> Vec<PollenDay> {
    let Some(times) = hourly.get("time").and_then(Value::as_array) else { return Vec::new() };
    let mut days: Vec<PollenDay> = Vec::new();
    for (i, t) in times.iter().enumerate() {
        let Some(date) = t.as_str().and_then(|t| t.get(..10)) else { continue };
        if days.last().is_none_or(|d| d.date != date) {
            days.push(PollenDay { date: date.to_string(), pollen: Vec::new() });
        }
        let day = days.last_mut().expect("just pushed");
        for p in POLLEN {
            let Some(v) = hourly.get(format!("{p}_pollen")).and_then(|s| s.get(i)).and_then(Value::as_f64) else {
                continue;
            };
            match day.pollen.iter_mut().find(|(k, _)| k == p) {
                Some((_, peak)) => *peak = peak.max(v),
                None => day.pollen.push((p.to_string(), v)),
            }
        }
    }
    days
}

// ── Normals ────────────────────────────────────────────────────────────

/// Downloads the daily highs and lows since 1991 (about 300 KB): averages
/// over 1991–2020 per day of the year, smoothed over ±7 days (the normals),
/// and each date's records up to last year.
pub async fn fetch_normals(location: &Location) -> Result<Normals, Error> {
    let this_year: i64 = crate::date::local_iso_minute(crate::cache::unix_now(), 0)[..4].parse().unwrap_or(2026);
    let url = format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={}&longitude={}\
         &start_date=1991-01-01&end_date={}-12-31&daily=temperature_2m_max,temperature_2m_min&timezone=auto",
        location.latitude,
        location.longitude,
        this_year - 1
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
    let mut record_max = vec![(f64::NEG_INFINITY, 0u16); 365];
    let mut record_min = vec![(f64::INFINITY, 0u16); 365];
    for (i, date) in raw.daily.time.iter().enumerate() {
        let (Some(day), Some(year), Some(hi), Some(lo)) = (
            day_of_year(date),
            date.get(..4).and_then(|y| y.parse::<u16>().ok()),
            raw.daily.temperature_2m_max.get(i).copied().flatten(),
            raw.daily.temperature_2m_min.get(i).copied().flatten(),
        ) else {
            continue;
        };
        // Records from every year; the normals from the 1991–2020 reference.
        if hi > record_max[day].0 {
            record_max[day] = (hi, year);
        }
        if lo < record_min[day].0 {
            record_min[day] = (lo, year);
        }
        if year > 2020 {
            continue;
        }
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
                    let j = (d + k).rem_euclid(365) as usize;
                    (s + sum[j], n + count[j])
                });
                if n > 0.0 { s / n } else { 0.0 }
            })
            .collect()
    };
    Ok(Normals { max: smooth(&sum_max), min: smooth(&sum_min), record_max, record_min })
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

/// Saved normals; files from before records existed count as missing, so
/// they're downloaded once more.
pub fn load_normals(dir: &Path, location: &Location) -> Option<Normals> {
    let n: Normals = serde_json::from_slice(&std::fs::read(dir.join(normals_file(location))).ok()?).ok()?;
    (!n.record_max.is_empty()).then_some(n)
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

/// Everything for one place: the forecast (required) with the ensemble's
/// runs and the air quality fetched alongside, and the normals (from disk
/// in `dir`, or downloaded once). Only the forecast's own failure is an error.
pub async fn download(dir: Option<&Path>, location: &Location, source: crate::Source) -> Result<Forecast, Error> {
    let warnings = async {
        let place = crate::warnings::with_areas(dir, location).await;
        crate::warnings::fetch_warnings(&place).await
    };
    let (forecast, air, warnings, ensemble) = tokio::join!(
        crate::api::fetch_forecast(location, source),
        fetch_air(location),
        warnings,
        fetch_ensemble(location)
    );
    let mut forecast = forecast?;
    if let Ok(ensemble) = ensemble {
        attach_ensemble(&mut forecast, ensemble);
    }
    // A failed warnings download leaves none rather than failing the forecast.
    forecast.warnings = warnings.unwrap_or_default();
    let normals = match dir {
        Some(dir) => normals_for(dir, location).await,
        None => None,
    };
    attach(&mut forecast, air.ok(), normals.as_ref());
    Ok(forecast)
}

/// Adds the extras to a forecast: air, and each day's normal and records.
pub fn attach(forecast: &mut Forecast, air: Option<Air>, normals: Option<&Normals>) {
    forecast.air = air;
    if let Some(n) = normals {
        for day in &mut forecast.days {
            if let Some(i) = day_of_year(&day.date) {
                day.normal_max = n.max.get(i).copied();
                day.normal_min = n.min.get(i).copied();
                day.record_max = n.record_max.get(i).copied().filter(|r| r.1 > 0);
                day.record_min = n.record_min.get(i).copied().filter(|r| r.1 > 0);
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
    fn ensemble_runs() {
        // Four runs: hour 1 wet in 3 of them; the day's totals 1.2, 0.5, 2.0, 0.0.
        let json = br#"{"hourly": {"time": ["2026-11-14T10:00", "2026-11-14T11:00"],
            "precipitation": [0.0, 1.2],
            "precipitation_member01": [0.0, 0.5],
            "precipitation_member02": [1.0, 1.0],
            "precipitation_member03": [0.0, 0.0],
            "temperature_2m": [9.0, 12.0],
            "temperature_2m_member01": [10.0, 11.0],
            "temperature_2m_member02": [8.0, 14.0],
            "temperature_2m_member03": [9.0, null]}}"#;
        let e = parse_ensemble(json).unwrap();
        assert_eq!(e.runs, 4);
        assert_eq!(e.hours["2026-11-14T10:00"], 0.25);
        assert_eq!(e.hours["2026-11-14T11:00"], 0.75);
        let day = e.days["2026-11-14"];
        assert_eq!((day.wet, day.runs), (2, 4), "two runs reach 1 mm");
        // Highs per run: 12, 11, 14, 9 → sorted 9, 11, 12, 14.
        let (low, mid, high) = day.high;
        assert!((low - 9.6).abs() < 1e-9 && (mid - 11.5).abs() < 1e-9 && (high - 13.4).abs() < 1e-9, "{:?}", day.high);
        // Hour 10: temperatures 8, 9, 9, 10; rain 0, 0, 0, 1.
        let s = &e.spread[0];
        assert_eq!((s.models, s.wet, s.t_mid), (4, 1, 9.0));
        assert!((s.t_min - 8.3).abs() < 1e-9 && (s.t_max - 9.7).abs() < 1e-9);
        assert!((s.r_max - 0.7).abs() < 1e-9 && s.r_min == 0.0);
    }

    #[test]
    fn quantiles_between_neighbours() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!((quantile(&v, 0.0), quantile(&v, 0.5), quantile(&v, 1.0)), (1.0, 3.0, 5.0));
        assert!((quantile(&v, 0.1) - 1.4).abs() < 1e-9);
        assert_eq!(quantile(&[7.0], 0.9), 7.0);
    }

    #[test]
    fn air_and_pollen() {
        let json = br#"{"current": {"european_aqi": 28, "pm2_5": 8.8, "birch_pollen": 12.0, "grass_pollen": null}}"#;
        let a = parse_air(json).unwrap();
        assert_eq!(a.european_aqi, Some(28.0));
        assert_eq!(a.pollen, vec![("birch".to_string(), 12.0)]);
        // Days keep each kind's peak.
        let json = br#"{"current": {"birch_pollen": 1.0},
            "hourly": {"time": ["2026-04-10T00:00", "2026-04-10T12:00", "2026-04-11T00:00"],
                       "birch_pollen": [3.0, 80.0, 5.0], "grass_pollen": [null, 2.0, 30.0]}}"#;
        let days = parse_air(json).unwrap().pollen_days;
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].date, "2026-04-10");
        assert_eq!(days[0].pollen, vec![("birch".to_string(), 80.0), ("grass".to_string(), 2.0)]);
        assert_eq!(days[1].pollen, vec![("birch".to_string(), 5.0), ("grass".to_string(), 30.0)]);
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
        // Records: the extreme of each date, from any year; normals 1991–2020.
        let json = br#"{"daily": {"time": ["1991-11-13", "2015-11-13", "2023-11-13"],
            "temperature_2m_max": [10.0, 17.0, 19.0], "temperature_2m_min": [2.0, -3.0, 4.0]}}"#;
        let n = parse_normals(json).unwrap();
        assert_eq!(n.record_max[d], (19.0, 2023));
        assert_eq!(n.record_min[d], (-3.0, 2015));
        assert_eq!(n.max[d], 13.5, "2023 is outside the normals' period");
    }
}
