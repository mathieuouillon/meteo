//! Which data source is most accurate where you live: each model's forecast
//! made the day before, compared with what happened.
//!
//! "What happened" is, in France, what the nearest Météo-France weather
//! stations measured (their SYNOP reports: temperature and 3-hour rain, every
//! 3 hours, about ten days late). Elsewhere it's the ERA5 reanalysis, which is
//! built with ECMWF's model and so favours ECMWF; the note under the list says
//! which was used.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{Error, client};
use crate::model::Location;
use crate::source::Source;

/// Stations within this distance count as "near".
const STATION_RADIUS_KM: u32 = 150;
/// At most this many stations, the nearest first.
const STATIONS: usize = 3;
/// Weeks of station reports asked for (they arrive about ten days late).
const STATION_DAYS: i64 = 35;
/// ERA5 arrives about five days late; three weeks of forecasts cover two.
const REANALYSIS_DAYS: u32 = 21;
/// A day with at least this much rain counts as a rainy day.
const WET_DAY_MM: f64 = 1.0;
/// Fewer temperatures or days than this compared and the score means nothing.
const MIN_TEMPS: usize = 40;
const MIN_DAYS: usize = 3;
/// Scores are worked out again after this long.
pub const MAX_AGE_SECS: i64 = 3 * 24 * 3600;
/// Saved scores from an older way of checking are worked out again.
const VERSION: u8 = 2;

/// How one source did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub source: Source,
    /// Average miss of the temperature, °C.
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
    /// The weather stations compared with ("Le Puy-Loudes"); empty when it
    /// was the ERA5 reanalysis.
    #[serde(default)]
    pub stations: Vec<String>,
    #[serde(default)]
    pub version: u8,
}

impl Accuracy {
    pub fn score(&self, source: Source) -> Option<&Score> {
        self.scores.iter().find(|s| s.source == source)
    }

    pub fn best(&self) -> Option<Source> {
        self.scores.first().map(|s| s.source)
    }
}

// ── What happened ──────────────────────────────────────────────────────

/// What happened at one place: temperatures at some UTC times
/// ("2026-11-03T21:00"), and each whole day's rain.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Truth {
    pub temps: HashMap<String, f64>,
    pub rain: HashMap<String, f64>,
}

/// The day an amount of rain measured up to `time` belongs to: rain
/// reported at 00:00 fell the day before.
fn rain_day(time: &str) -> Option<String> {
    let unix = crate::date::parse_rfc3339(time)? - 1;
    Some(crate::date::local_iso_minute(unix, 0)[..10].to_string())
}

/// Sums amounts per day, keeping only days with all `per_day` readings.
fn whole_days(amounts: impl Iterator<Item = (String, f64)>, per_day: usize) -> HashMap<String, f64> {
    let mut days: HashMap<String, (f64, usize)> = HashMap::new();
    for (time, mm) in amounts {
        if let Some(day) = rain_day(&time) {
            let d = days.entry(day).or_default();
            (d.0, d.1) = (d.0 + mm, d.1 + 1);
        }
    }
    days.into_iter().filter(|(_, (_, n))| *n == per_day).map(|(day, (mm, _))| (day, mm)).collect()
}

impl Truth {
    /// From one station's SYNOP reports (Opendatasoft records): temperature
    /// in kelvin, `rr3` the rain of the 3 hours before (−0.1 for a trace).
    pub fn from_station(records: &[&Value]) -> Self {
        let time = |r: &Value| r["date"].as_str().and_then(|d| d.get(..16)).map(str::to_owned);
        let temps = records.iter().filter_map(|r| Some((time(r)?, r["t"].as_f64()? - 273.15))).collect();
        let rain = whole_days(records.iter().filter_map(|r| Some((time(r)?, r["rr3"].as_f64()?.max(0.0)))), 8);
        Truth { temps, rain }
    }

    /// From an Open-Meteo archive answer (hourly, the hour's rain ending at
    /// each time).
    pub fn from_reanalysis(json: &[u8]) -> Result<Self, Error> {
        let v: Value = serde_json::from_slice(json)?;
        let h = &v["hourly"];
        let times = h["time"].as_array().ok_or(Error::MissingData)?;
        let at = |key: &str, i: usize| h[key].get(i).and_then(Value::as_f64);
        let series = |key: &str| -> Vec<(String, f64)> {
            times.iter().enumerate().filter_map(|(i, t)| Some((t.as_str()?.to_owned(), at(key, i)?))).collect()
        };
        Ok(Truth {
            temps: series("temperature_2m").into_iter().collect(),
            rain: whole_days(series("precipitation").into_iter(), 24),
        })
    }
}

// ── Scoring ────────────────────────────────────────────────────────────

/// Running totals for one source, over one or several places.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    miss: f64,
    temps: usize,
    right: usize,
    days: usize,
}

#[derive(Debug, Default)]
struct Tallies {
    by_source: HashMap<Source, Tally>,
    first_day: Option<String>,
    last_day: Option<String>,
}

impl Tallies {
    /// Adds one place: the sources' day-before forecasts there (an Open-Meteo
    /// previous-runs answer, same time zone as `truth`) against what happened.
    fn add(&mut self, forecasts: &[u8], truth: &Truth) -> Result<(), Error> {
        let f: Value = serde_json::from_slice(forecasts)?;
        let h = &f["hourly"];
        let times: Vec<&str> =
            h["time"].as_array().ok_or(Error::MissingData)?.iter().filter_map(Value::as_str).collect();
        for source in Source::ALL {
            let id = source.model_id();
            let temp = &h[format!("temperature_2m_previous_day1_{id}")];
            let rain = &h[format!("precipitation_previous_day1_{id}")];
            let tally = self.by_source.entry(source).or_default();
            for (i, t) in times.iter().enumerate() {
                if let (Some(seen), Some(said)) = (truth.temps.get(*t), temp.get(i).and_then(Value::as_f64)) {
                    tally.miss += (said - seen).abs();
                    tally.temps += 1;
                }
            }
            let said = whole_days(
                times.iter().enumerate().filter_map(|(i, t)| Some((t.to_string(), rain.get(i)?.as_f64()?))),
                24,
            );
            for (day, seen) in &truth.rain {
                let Some(said) = said.get(day) else { continue };
                tally.right += usize::from((*said >= WET_DAY_MM) == (*seen >= WET_DAY_MM));
                tally.days += 1;
                if self.first_day.as_ref().is_none_or(|d| day < d) {
                    self.first_day = Some(day.clone());
                }
                if self.last_day.as_ref().is_none_or(|d| day > d) {
                    self.last_day = Some(day.clone());
                }
            }
        }
        Ok(())
    }

    fn finish(self, now: i64, stations: Vec<String>) -> Result<Accuracy, Error> {
        let mut scores: Vec<Score> = self
            .by_source
            .into_iter()
            .filter(|(_, t)| t.temps >= MIN_TEMPS && t.days >= MIN_DAYS)
            .map(|(source, t)| Score {
                source,
                temp_error: (t.miss / t.temps as f64 * 10.0).round() / 10.0,
                rain_right: t.right.min(255) as u8,
                days: t.days.min(255) as u8,
            })
            .collect();
        if scores.is_empty() {
            return Err(Error::MissingData);
        }
        // Ties keep the list's order.
        scores.sort_by(|a, b| a.penalty().total_cmp(&b.penalty()).then(index(a.source).cmp(&index(b.source))));
        Ok(Accuracy {
            scores,
            from: self.first_day.ok_or(Error::MissingData)?,
            to: self.last_day.ok_or(Error::MissingData)?,
            computed_at: now,
            stations,
            version: VERSION,
        })
    }
}

fn index(source: Source) -> usize {
    Source::ALL.iter().position(|&s| s == source).unwrap_or(usize::MAX)
}

/// One request for every source's day-before forecasts at a place.
fn forecasts_url(latitude: f64, longitude: f64, elevation: Option<f64>, when: &str, time_zone: &str) -> String {
    let models: Vec<&str> = Source::ALL.iter().map(|s| s.model_id()).collect();
    let elevation = elevation.map(|e| format!("&elevation={e}")).unwrap_or_default();
    format!(
        "https://previous-runs-api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}{elevation}\
         &hourly=temperature_2m_previous_day1,precipitation_previous_day1{when}&timezone={time_zone}&models={}",
        models.join(",")
    )
}

async fn get(url: &str) -> Result<Vec<u8>, Error> {
    Ok(client()?.get(url).send().await?.error_for_status()?.bytes().await?.to_vec())
}

// ── Stations (France) ──────────────────────────────────────────────────

/// A Météo-France station with its recent reports.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: Option<f64>,
    pub records: Vec<Value>,
}

/// The nearest stations (within `STATION_RADIUS_KM`) with enough reports,
/// nearest first, from Opendatasoft's SYNOP records.
pub fn parse_stations(json: &[u8], latitude: f64, longitude: f64) -> Result<Vec<Station>, Error> {
    let records: Vec<Value> = serde_json::from_slice(json)?;
    let mut by_id: HashMap<String, Station> = HashMap::new();
    for r in records {
        let Some(id) = r["numer_sta"].as_str().map(str::to_owned) else { continue };
        let station = by_id.entry(id).or_insert_with(|| Station {
            name: pretty_name(r["nom"].as_str().unwrap_or("?")),
            latitude: r["coordonnees"]["lat"].as_f64().unwrap_or(f64::NAN),
            longitude: r["coordonnees"]["lon"].as_f64().unwrap_or(f64::NAN),
            altitude: r["altitude"].as_f64(),
            records: Vec::new(),
        });
        station.records.push(r);
    }
    let mut stations: Vec<Station> = by_id
        .into_values()
        .filter(|s| s.records.len() >= 100 && s.latitude.is_finite() && s.longitude.is_finite())
        .collect();
    stations.sort_by(|a, b| {
        distance_km(latitude, longitude, a.latitude, a.longitude).total_cmp(&distance_km(
            latitude,
            longitude,
            b.latitude,
            b.longitude,
        ))
    });
    stations.truncate(STATIONS);
    Ok(stations)
}

/// "LE PUY-LOUDES" → "Le Puy-Loudes", "ST GIRONS" → "St Girons".
fn pretty_name(name: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in name.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start = !c.is_alphanumeric() && c != '\'';
    }
    out
}

fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().asin()
}

async fn stations_near(location: &Location, now: i64) -> Result<Vec<Station>, Error> {
    let since = crate::date::local_iso_minute(now - STATION_DAYS * 86_400, 0);
    let filter = format!(
        "within_distance(coordonnees, geom'POINT({} {})', {STATION_RADIUS_KM}km) and date >= '{}'",
        location.longitude,
        location.latitude,
        &since[..10]
    );
    let url = reqwest::Url::parse_with_params(
        "https://public.opendatasoft.com/api/explore/v2.1/catalog/datasets/donnees-synop-essentielles-omm/exports/json",
        &[("select", "numer_sta,nom,date,t,rr3,coordonnees,altitude"), ("where", filter.as_str())],
    )
    .expect("valid stations URL");
    parse_stations(&get(url.as_str()).await?, location.latitude, location.longitude)
}

/// Each source's forecasts at each station (its spot and height), against
/// what the station measured.
async fn with_stations(stations: Vec<Station>, now: i64) -> Result<Accuracy, Error> {
    let mut tallies = Tallies::default();
    let mut names = Vec::new();
    for s in stations {
        let records: Vec<&Value> = s.records.iter().collect();
        let truth = Truth::from_station(&records);
        let mut days: Vec<&String> = truth.temps.keys().collect();
        days.sort();
        let (Some(first), Some(last)) = (days.first(), days.last()) else { continue };
        let when = format!("&start_date={}&end_date={}", &first[..10], &last[..10]);
        let url = forecasts_url(s.latitude, s.longitude, s.altitude, &when, "GMT");
        if let Ok(forecasts) = get(&url).await
            && tallies.add(&forecasts, &truth).is_ok()
        {
            names.push(s.name);
        }
    }
    tallies.finish(now, names)
}

// ── Reanalysis (elsewhere) ─────────────────────────────────────────────

async fn with_reanalysis(location: &Location, now: i64) -> Result<Accuracy, Error> {
    let when = format!("&past_days={REANALYSIS_DAYS}&forecast_days=1");
    let forecasts = get(&forecasts_url(location.latitude, location.longitude, None, &when, "GMT")).await?;
    let start = crate::date::local_iso_minute(now - i64::from(REANALYSIS_DAYS) * 86_400, 0);
    let end = crate::date::local_iso_minute(now, 0);
    let truth = format!(
        "https://archive-api.open-meteo.com/v1/archive?latitude={}&longitude={}\
         &hourly=temperature_2m,precipitation&start_date={}&end_date={}&models=era5_seamless&timezone=GMT",
        location.latitude,
        location.longitude,
        &start[..10],
        &end[..10]
    );
    let truth = Truth::from_reanalysis(&get(&truth).await?)?;
    let mut tallies = Tallies::default();
    tallies.add(&forecasts, &truth)?;
    tallies.finish(now, Vec::new())
}

/// Stations when there are some near (France), the reanalysis otherwise.
pub async fn fetch(location: &Location, now: i64) -> Result<Accuracy, Error> {
    if let Ok(stations) = stations_near(location, now).await
        && !stations.is_empty()
        && let Ok(a) = with_stations(stations, now).await
    {
        return Ok(a);
    }
    with_reanalysis(location, now).await
}

fn file(location: &Location) -> String {
    format!("accuracy-{}.json", location.key())
}

pub fn load(dir: &Path, location: &Location) -> Option<Accuracy> {
    let a: Accuracy = serde_json::from_slice(&std::fs::read(dir.join(file(location))).ok()?).ok()?;
    (a.version == VERSION).then_some(a)
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
    use serde_json::json;

    /// Hourly UTC times over `days` days from 1 November, plus the next
    /// midnight (which closes the last day's rain).
    fn hours(days: usize) -> Vec<String> {
        (0..=days * 24).map(|h| format!("2026-11-{:02}T{:02}:00", 1 + h / 24, h % 24)).collect()
    }

    /// A previous-runs answer: ECMWF 1° too warm with the rain right, ICON 3°
    /// too cold and always dry.
    fn forecasts(times: &[String], temp: impl Fn(usize) -> f64, rain: impl Fn(usize) -> f64) -> String {
        let n = times.len();
        let t = |by: f64| (0..n).map(|i| temp(i) + by).collect::<Vec<_>>();
        json!({"hourly": {
            "time": times,
            "temperature_2m_previous_day1_ecmwf_ifs025": t(1.0),
            "precipitation_previous_day1_ecmwf_ifs025": (0..n).map(&rain).collect::<Vec<_>>(),
            "temperature_2m_previous_day1_icon_seamless": t(-3.0),
            "precipitation_previous_day1_icon_seamless": vec![0.0; n],
        }})
        .to_string()
    }

    #[test]
    fn rain_belongs_to_the_hour_before() {
        assert_eq!(rain_day("2026-11-02T00:00").as_deref(), Some("2026-11-01"));
        assert_eq!(rain_day("2026-11-02T03:00").as_deref(), Some("2026-11-02"));
    }

    #[test]
    fn scores_against_a_station() {
        // 6 days of 3-hourly reports: 10 °C, and 4 mm in the 3 hours to
        // 2 November 06:00 (a rainy day; the others dry).
        let times = hours(6);
        let records: Vec<Value> = times
            .iter()
            .step_by(3)
            .map(|t| {
                let rr3 = if t == "2026-11-02T06:00" { 4.0 } else { -0.1 };
                json!({"numer_sta": "07471", "nom": "LE PUY-LOUDES", "date": format!("{t}:00+00:00"),
                       "t": 283.15, "rr3": rr3, "coordonnees": {"lat": 45.07, "lon": 3.76}, "altitude": 833})
            })
            .collect();
        let truth = Truth::from_station(&records.iter().collect::<Vec<_>>());
        assert_eq!(truth.rain.len(), 6, "the 6 whole days");
        assert_eq!(truth.rain["2026-11-02"], 4.0);
        assert_eq!(truth.rain["2026-11-01"], 0.0, "a trace is no rain");
        assert!((truth.temps["2026-11-01T03:00"] - 10.0).abs() < 1e-9);

        let f = forecasts(&times, |_| 10.0, |i| if times[i] == "2026-11-02T05:00" { 3.0 } else { 0.0 });
        let mut tallies = Tallies::default();
        tallies.add(f.as_bytes(), &truth).unwrap();
        let a = tallies.finish(7, vec!["Le Puy-Loudes".into()]).unwrap();
        assert_eq!(a.best(), Some(Source::Ecmwf));
        let ecmwf = a.score(Source::Ecmwf).unwrap();
        assert_eq!((ecmwf.temp_error, ecmwf.rain_right, ecmwf.days), (1.0, 6, 6));
        let icon = a.score(Source::Dwd).unwrap();
        assert_eq!((icon.temp_error, icon.rain_right, icon.days), (3.0, 5, 6));
        assert_eq!((a.from.as_str(), a.to.as_str()), ("2026-11-01", "2026-11-06"));
        assert_eq!((a.stations.clone(), a.version), (vec!["Le Puy-Loudes".to_string()], VERSION));
        assert!(a.score(Source::Noaa).is_none(), "no data, no score");
    }

    #[test]
    fn too_little_says_nothing() {
        let times = hours(1);
        let f = forecasts(&times, |_| 10.0, |_| 0.0);
        let truth = Truth {
            temps: times.iter().map(|t| (t.clone(), 10.0)).collect(),
            rain: [("2026-11-01".to_string(), 0.0)].into(),
        };
        let mut tallies = Tallies::default();
        tallies.add(f.as_bytes(), &truth).unwrap();
        assert!(tallies.finish(0, Vec::new()).is_err(), "one day only");
    }

    #[test]
    fn reanalysis_days_are_whole() {
        let times = hours(2);
        let json = json!({"hourly": {"time": times, "temperature_2m": vec![5.0; times.len()],
                                     "precipitation": vec![0.5; times.len()]}});
        let t = Truth::from_reanalysis(json.to_string().as_bytes()).unwrap();
        assert_eq!(t.rain.len(), 2, "1 and 2 November; 31 October has a single hour");
        assert!((t.rain["2026-11-01"] - 12.0).abs() < 1e-9);
    }

    #[test]
    fn nearest_stations_first() {
        let rec = |id: &str, name: &str, lat: f64, lon: f64, n: usize| -> Vec<Value> {
            (0..n)
                .map(
                    |_| json!({"numer_sta": id, "nom": name, "coordonnees": {"lat": lat, "lon": lon}, "altitude": 200}),
                )
                .collect()
        };
        let mut all = rec("1", "LYON-ST EXUPERY", 45.73, 5.08, 120);
        all.extend(rec("2", "LE PUY-LOUDES", 45.07, 3.76, 120));
        all.extend(rec("3", "CLERMONT-FD", 45.79, 3.15, 50)); // too few reports
        let s = parse_stations(serde_json::to_string(&all).unwrap().as_bytes(), 45.47, 4.34).unwrap();
        let names: Vec<&str> = s.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Le Puy-Loudes", "Lyon-St Exupery"]);
        assert_eq!(pretty_name("ST GIRONS"), "St Girons");
        assert_eq!(pretty_name("PTE DE LA HAGUE"), "Pte De La Hague");
    }

    #[test]
    fn old_saved_scores_are_redone() {
        let dir = std::env::temp_dir().join(format!("meteo-accuracy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let place = Location::paris();
        let old = r#"{"scores":[],"from":"a","to":"b","computed_at":1}"#;
        std::fs::write(dir.join(file(&place)), old).unwrap();
        assert!(load(&dir, &place).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_saint_etienne_and_new_york() {
        for place in [Location::new("La Fouillouse", "", 45.5, 4.31), Location::new("New York", "", 40.71, -74.01)] {
            let a = fetch(&place, crate::cache::unix_now()).await.unwrap();
            println!("{}: {} → {} {:?}", place.name, a.from, a.to, a.stations);
            for s in &a.scores {
                println!("  {:?}: ±{}° rain {}/{}", s.source, s.temp_error, s.rain_right, s.days);
            }
        }
    }
}
