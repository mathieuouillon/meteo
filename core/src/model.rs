use serde::Serialize;

use crate::condition::Condition;

/// A place to fetch weather for.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Location {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    pub fn paris() -> Self {
        Self {
            name: "Paris".into(),
            latitude: 48.8566,
            longitude: 2.3522,
        }
    }
}

/// Everything the main screen shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Forecast {
    pub location: String,
    pub current: Current,
    pub today_min: f64,
    pub today_max: f64,
    /// The next 24 hours, starting with the current hour.
    pub hours: Vec<Hour>,
    /// Today and the following days (7 in total).
    pub days: Vec<Day>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Current {
    /// Local time, e.g. "2026-11-12T14:15".
    pub time: String,
    pub temperature: f64,
    pub apparent_temperature: f64,
    pub humidity: f64,
    pub wind_speed: f64,
    pub condition: Condition,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hour {
    /// Local time, e.g. "2026-11-12T15:00".
    pub time: String,
    /// Short label for the UI, e.g. "15h" (or "Now" for the first hour).
    pub label: String,
    pub temperature: f64,
    /// 0–100 %.
    pub precipitation_probability: f64,
    pub condition: Condition,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Day {
    /// Local date, e.g. "2026-11-12".
    pub date: String,
    /// "Today", then short weekday names ("Fri").
    pub label: String,
    pub min: f64,
    pub max: f64,
    /// Highest hourly chance of precipitation that day, 0–100 %.
    pub precipitation_probability: f64,
    pub condition: Condition,
}
