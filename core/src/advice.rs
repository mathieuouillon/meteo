//! One short, practical sentence for the rest of the day, under the
//! temperature: the most useful of rain, snow, gusts, heat, frost tonight and
//! strong sun, in that order. Built from the hourly forecast.

use crate::condition::Condition;
use crate::i18n::Lang;
use crate::model::{Forecast, Hour};
use crate::units::Units;

/// Rain worth an umbrella: a real amount, or a likely shower.
const WET_MM: f64 = 0.2;
const WET_CHANCE: f64 = 60.0;
const GUSTS_KMH: f64 = 60.0;
const HOT_C: f64 = 30.0;
const UV_HIGH: f64 = 6.0;

/// The advice for now, or `None` when there's nothing to say.
pub fn advice(f: &Forecast, lang: Lang, units: Units) -> Option<String> {
    let today = f.current.time.get(..10)?;
    let now_hour: u32 = f.current.time.get(11..13)?.parse().ok()?;
    // The rest of today, from the current hour.
    let rest: Vec<&Hour> = f.hours.iter().take_while(|h| h.time.starts_with(today)).collect();
    if rest.is_empty() {
        return None;
    }
    let clock = |h: &Hour| format!("{}h", h.time.get(11..13).unwrap_or("?"));
    let wet = |h: &&Hour| h.precipitation >= WET_MM || h.precipitation_probability >= WET_CHANCE;

    // Rain or snow: when it starts, or until when it lasts.
    if let Some(first) = rest.iter().find(|h| wet(h)) {
        let snow = first.condition == Condition::Snow;
        let raining_now = wet(&rest[0]);
        return Some(if raining_now {
            match rest.iter().find(|h| !wet(h)) {
                Some(dry) => lang.advice_until(snow, &clock(dry)),
                None => lang.advice_all_day(snow),
            }
        } else {
            lang.advice_from(snow, &clock(first))
        });
    }

    // Strong gusts.
    if let Some(peak) = rest.iter().filter_map(|h| h.wind_gusts).filter(|g| *g >= GUSTS_KMH).reduce(f64::max) {
        return Some(lang.advice_gusts(&units.wind_text(peak)));
    }

    // Heat: the day's peak, if it's still to come.
    if let Some(hot) =
        rest.iter().filter(|h| h.temperature >= HOT_C).max_by(|a, b| a.temperature.total_cmp(&b.temperature))
    {
        return Some(lang.advice_heat(&units.temp(hot.temperature), &clock(hot)));
    }

    // Frost tonight, from midday on: the night until 9 am tomorrow.
    if now_hour >= 12 {
        let night = f.hours.iter().take_while(|h| h.time.starts_with(today) || h.time.get(11..13) < Some("10"));
        if night.map(|h| h.temperature).reduce(f64::min).is_some_and(|t| t <= 0.0) {
            return Some(lang.advice_frost().into());
        }
    }

    // Strong sun: the hours with a high UV index.
    let sunny: Vec<&&Hour> = rest.iter().filter(|h| h.uv_index.is_some_and(|u| u >= UV_HIGH)).collect();
    if let (Some(first), Some(last)) = (sunny.first(), sunny.last()) {
        let peak = sunny.iter().filter_map(|h| h.uv_index).reduce(f64::max).unwrap_or(UV_HIGH);
        let end: u32 = last.time.get(11..13).and_then(|h| h.parse().ok()).unwrap_or(0) + 1;
        return Some(lang.advice_uv(&clock(first), &format!("{end}h"), peak.round() as u32));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Current;

    fn forecast(now: &str, hours: Vec<Hour>) -> Forecast {
        let mut f = crate::api::parse_forecast(
            "Lyon",
            br#"{"utc_offset_seconds":3600,"current":{"time":"2026-11-14T09:00","temperature_2m":7,"apparent_temperature":5,"relative_humidity_2m":80,"is_day":1,"weather_code":3,"wind_speed_10m":10},
            "hourly":{"time":["2026-11-14T09:00"],"temperature_2m":[7],"weather_code":[3],"is_day":[1]},
            "daily":{"time":["2026-11-14"],"weather_code":[3],"temperature_2m_max":[10],"temperature_2m_min":[5]}}"#,
        )
        .unwrap();
        f.current = Current { time: now.into(), ..f.current };
        f.hours = hours;
        f
    }

    fn hour(time: &str, t: f64, mm: f64) -> Hour {
        Hour {
            time: time.into(),
            label: String::new(),
            temperature: t,
            precipitation_probability: 0.0,
            precipitation: mm,
            condition: if mm > 0.0 { Condition::Rain } else { Condition::Clear },
            is_day: true,
            wind_gusts: None,
            uv_index: None,
        }
    }

    fn day(from: u32, t: f64, rain_at: &[u32]) -> Vec<Hour> {
        (from..24)
            .map(|h| hour(&format!("2026-11-14T{h:02}:00"), t, if rain_at.contains(&h) { 1.0 } else { 0.0 }))
            .chain((0..10).map(|h| hour(&format!("2026-11-15T{h:02}:00"), t, 0.0)))
            .collect()
    }

    fn say(f: &Forecast) -> Option<String> {
        advice(f, Lang::En, Units::Metric)
    }

    #[test]
    fn rain_coming_raining_and_all_day() {
        assert_eq!(say(&forecast("2026-11-14T09:00", day(9, 12.0, &[16, 17]))).unwrap(), "Take an umbrella from 16h");
        assert_eq!(say(&forecast("2026-11-14T09:00", day(9, 12.0, &[9, 10, 11]))).unwrap(), "Rain until 12h");
        let all: Vec<u32> = (9..24).collect();
        assert_eq!(say(&forecast("2026-11-14T09:00", day(9, 12.0, &all))).unwrap(), "Rain all day: take an umbrella");
        let mut snow = day(9, -1.0, &[15]);
        snow[6].condition = Condition::Snow;
        assert_eq!(say(&forecast("2026-11-14T09:00", snow)).unwrap(), "Snow from 15h");
    }

    #[test]
    fn gusts_heat_frost_and_sun() {
        let mut gusty = day(9, 12.0, &[]);
        gusty[5].wind_gusts = Some(72.0);
        assert_eq!(say(&forecast("2026-11-14T09:00", gusty)).unwrap(), "Gusts up to 72 km/h today");
        let mut hot = day(9, 25.0, &[]);
        hot[7].temperature = 32.0;
        assert_eq!(say(&forecast("2026-11-14T09:00", hot)).unwrap(), "Hot: up to 32° at 16h, drink water");
        let mut cold = day(14, 6.0, &[]);
        let n = cold.len();
        cold[n - 4].temperature = -2.0; // 06h tomorrow
        assert_eq!(say(&forecast("2026-11-14T14:00", cold.clone())).unwrap(), "Frost tonight: cover plants");
        assert_eq!(say(&forecast("2026-11-14T09:00", cold)), None, "frost is an afternoon/evening tip");
        let mut sunny = day(9, 22.0, &[]);
        for (i, uv) in [(3, 6.0), (4, 7.4), (5, 6.5)] {
            sunny[i].uv_index = Some(uv);
        }
        assert_eq!(say(&forecast("2026-11-14T09:00", sunny)).unwrap(), "Sunscreen 12h–15h (UV 7)");
        assert_eq!(say(&forecast("2026-11-14T09:00", day(9, 15.0, &[]))), None);
        assert_eq!(
            advice(&forecast("2026-11-14T09:00", day(9, 12.0, &[16])), Lang::Fr, Units::Metric).unwrap(),
            "Parapluie à partir de 16h"
        );
    }
}
