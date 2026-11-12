use serde::{Deserialize, Serialize};

use crate::condition::Condition;

/// A place to fetch weather for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub name: String,
    /// Region and country, e.g. "Île-de-France, France" (may be empty).
    #[serde(default)]
    pub detail: String,
    pub latitude: f64,
    pub longitude: f64,
}

impl Location {
    pub fn paris() -> Self {
        Self {
            name: "Paris".into(),
            detail: "Île-de-France, France".into(),
            latitude: 48.8566,
            longitude: 2.3522,
        }
    }

    /// Identifies the place by its coordinates (about 1 km precision), for
    /// cache file names and to spot duplicates.
    pub fn key(&self) -> String {
        format!("{:.2}_{:.2}", self.latitude, self.longitude)
    }
}

/// Everything the main screen shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Forecast {
    pub location: String,
    /// Offset of the location's local time from UTC, so a cached forecast
    /// can be moved forward to "now" without a time-zone database.
    pub utc_offset_seconds: i64,
    pub current: Current,
    pub today_min: f64,
    pub today_max: f64,
    /// Upcoming hours (48 kept), starting with the current hour. Screens
    /// show the first 24; the extra ones keep a cached forecast useful.
    pub hours: Vec<Hour>,
    /// Today and the following days (7 in total).
    pub days: Vec<Day>,
}

impl Forecast {
    /// Drops hours and days that are already over at `unix_now`, and
    /// relabels the first ones "Now" and "Today". Used when showing a
    /// cached forecast. The "current" conditions are left as they were.
    pub fn advance_to(&mut self, unix_now: i64) {
        let now = crate::date::local_iso_minute(unix_now, self.utc_offset_seconds);
        let (this_hour, today) = (&now[..13], &now[..10]);

        self.hours.retain(|h| h.time.get(..13).is_some_and(|t| t >= this_hour));
        for (i, h) in self.hours.iter_mut().enumerate() {
            h.label = if i == 0 {
                "Now".into()
            } else {
                format!("{}h", h.time.get(11..13).unwrap_or("?"))
            };
        }

        self.days.retain(|d| d.date.as_str() >= today);
        for (i, d) in self.days.iter_mut().enumerate() {
            d.label = if i == 0 {
                "Today".into()
            } else {
                crate::date::weekday_short(&d.date).unwrap_or("?").into()
            };
        }
        if let Some(d) = self.days.first() {
            (self.today_min, self.today_max) = (d.min, d.max);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
