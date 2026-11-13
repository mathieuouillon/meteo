//! Which notifications to post, decided from the cached forecast and the
//! settings. The Android side wakes up every 15 minutes, refreshes the
//! forecast, asks this, and posts what comes back. What was already sent is
//! remembered in `notify-state.json`, so nothing repeats.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cache;
use crate::date::local_iso_minute;
use crate::i18n::Lang;
use crate::places::Places;
use crate::source::Settings;
use crate::view::MainScreen;

const STATE_FILE: &str = "notify-state.json";
/// The morning summary goes out the first time we wake up in this window.
const MORNING: (u32, u32) = (7, 10);
/// No second rain alert within this time.
const RAIN_ALERT_GAP_SECS: i64 = 3 * 3600;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Notification {
    /// "morning" or "rain" (also the Android channel).
    pub kind: String,
    pub title: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct State {
    /// Local date of the last morning summary, "2026-11-13".
    #[serde(default)]
    morning_date: String,
    #[serde(default)]
    rain_alert_at: i64,
}

/// The notifications due now (usually none). `system_lang` is the phone's.
pub fn due(dir: &Path, unix_now: i64, system_lang: Lang) -> Vec<Notification> {
    let settings = Settings::load(dir);
    if !settings.morning_summary && !settings.rain_alerts {
        return Vec::new();
    }
    let lang = settings.lang(system_lang);
    let city = Places::load(dir).selected().clone();
    let Some(cached) = cache::load(dir, &city, settings.source, unix_now) else {
        return Vec::new();
    };
    let f = &cached.forecast;
    let screen = MainScreen::from_forecast(f, lang, settings.units);
    let local = local_iso_minute(unix_now, f.utc_offset_seconds);
    let hour: u32 = local.get(11..13).and_then(|h| h.parse().ok()).unwrap_or(0);
    let today = local.get(..10).unwrap_or("").to_string();

    let mut state: State = std::fs::read(dir.join(STATE_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let mut out = Vec::new();

    if settings.morning_summary && (MORNING.0..MORNING.1).contains(&hour) && state.morning_date != today {
        let mut text = format!("{} · {}", screen.condition, screen.high_low);
        if let Some(day) = screen.days.first().filter(|d| !d.rain_total.is_empty()) {
            text += &format!(" · {}", day.rain_total);
        }
        if !screen.nowcast.summary.is_empty() && screen.nowcast.wet {
            text += &format!("\n{}", screen.nowcast.summary);
        }
        out.push(Notification { kind: "morning".into(), title: lang.morning_title(&city.name), text });
        state.morning_date = today;
    }

    // Rain about to start (not already raining), at most every few hours.
    let soon = screen.nowcast.starts_in.is_some_and(|m| (1..=60).contains(&m));
    if settings.rain_alerts && soon && unix_now - state.rain_alert_at > RAIN_ALERT_GAP_SECS {
        out.push(Notification { kind: "rain".into(), title: lang.rain_soon_title(&city.name), text: screen.nowcast.summary.clone() });
        state.rain_alert_at = unix_now;
    }

    if !out.is_empty() {
        if let Ok(json) = serde_json::to_vec(&state) {
            let _ = cache::write_atomic(dir, STATE_FILE, &json);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::parse_forecast;
    use crate::model::{Location, Quarter};
    use crate::source::Source;

    /// 2026-11-13 08:00 in Paris (UTC+1).
    const MORNING_8AM: i64 = 1_794_553_200;

    fn setup(name: &str, morning: bool, rain: bool, rain_in: Option<usize>) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("meteo-notify-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Settings { morning_summary: morning, rain_alerts: rain, ..Settings::default() }.save(&dir).unwrap();
        let json = br#"{"utc_offset_seconds":3600,"current":{"time":"2026-11-13T08:00","temperature_2m":7,"apparent_temperature":5,"relative_humidity_2m":80,"is_day":1,"weather_code":3,"wind_speed_10m":10},
            "hourly":{"time":["2026-11-13T08:00","2026-11-13T09:00"],"temperature_2m":[7,8],"weather_code":[3,61],"is_day":[1,1]},
            "daily":{"time":["2026-11-13"],"weather_code":[61],"temperature_2m_max":[10],"temperature_2m_min":[5],"precipitation_sum":[2.4]}}"#;
        let mut f = parse_forecast("Paris", json).unwrap();
        f.nowcast = (0..8)
            .map(|i| Quarter {
                time: format!("2026-11-13T{:02}:{:02}", 8 + i / 4, i % 4 * 15),
                precipitation: if rain_in.is_some_and(|k| i >= k) { 0.3 } else { 0.0 },
            })
            .collect();
        cache::save(&dir, &Location::paris(), Source::BestMatch, &f, MORNING_8AM).unwrap();
        dir
    }

    #[test]
    fn morning_summary_once_a_day() {
        let dir = setup("morning", true, false, None);
        let first = due(&dir, MORNING_8AM, Lang::En);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, "morning");
        assert_eq!(first[0].text, "Cloudy · Day 10° · Night 5° · 2.4 mm");
        assert!(due(&dir, MORNING_8AM + 900, Lang::En).is_empty(), "not twice the same day");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rain_alert_when_rain_is_coming() {
        let dir = setup("rain", false, true, Some(2));
        let alerts = due(&dir, MORNING_8AM, Lang::Fr);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, "rain");
        assert!(alerts[0].text.contains("30 min"), "{}", alerts[0].text);
        assert!(due(&dir, MORNING_8AM + 900, Lang::Fr).is_empty(), "not again soon after");
        std::fs::remove_dir_all(&dir).unwrap();
        // Already raining, or off: nothing.
        let dir = setup("raining", false, true, Some(0));
        assert!(due(&dir, MORNING_8AM, Lang::En).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
