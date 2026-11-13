//! Official weather warnings from [MeteoAlarm](https://meteoalarm.org): the
//! national weather services of about 38 European countries, including
//! Météo-France's vigilance. There is one Atom feed per country; a place's
//! warnings are the entries whose area matches one of its administrative
//! areas (département, district, region…) or its own name.
//!
//! Only the country goes to MeteoAlarm, never the place's coordinates.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::api::{Error, client};
use crate::date::parse_rfc3339;
use crate::model::Location;

/// One warning for one area.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    /// MeteoAlarm's awareness type, lowercase: "thunderstorm", "wind",
    /// "rain-flood", "snow-ice", "avalanches"…
    pub kind: String,
    /// 1 yellow (be aware), 2 orange (be prepared), 3 red (take action).
    pub level: u8,
    /// The area as the feed names it ("Isère").
    pub area: String,
    /// Unix seconds.
    pub onset: i64,
    pub expires: i64,
    /// The CAP identifier, so a notification is sent once.
    pub id: String,
}

impl Warning {
    pub fn is_current(&self, unix_now: i64) -> bool {
        self.expires > unix_now
    }
}

/// MeteoAlarm's feed name for an ISO country code.
pub fn feed_name(country_code: &str) -> Option<&'static str> {
    Some(match country_code.to_ascii_uppercase().as_str() {
        "AT" => "austria",
        "BA" => "bosnia-herzegovina",
        "BE" => "belgium",
        "BG" => "bulgaria",
        "CH" => "switzerland",
        "CY" => "cyprus",
        "CZ" => "czechia",
        "DE" => "germany",
        "DK" => "denmark",
        "EE" => "estonia",
        "ES" => "spain",
        "FI" => "finland",
        "FR" => "france",
        "GB" => "united-kingdom",
        "GR" => "greece",
        "HR" => "croatia",
        "HU" => "hungary",
        "IE" => "ireland",
        "IL" => "israel",
        "IS" => "iceland",
        "IT" => "italy",
        "LT" => "lithuania",
        "LU" => "luxembourg",
        "LV" => "latvia",
        "MD" => "moldova",
        "ME" => "montenegro",
        "MK" => "republic-of-north-macedonia",
        "MT" => "malta",
        "NL" => "netherlands",
        "NO" => "norway",
        "PL" => "poland",
        "PT" => "portugal",
        "RO" => "romania",
        "RS" => "serbia",
        "SE" => "sweden",
        "SI" => "slovenia",
        "SK" => "slovakia",
        "UA" => "ukraine",
        _ => return None,
    })
}

/// The warnings for `location` (empty outside MeteoAlarm's countries).
pub async fn fetch_warnings(location: &Location) -> Result<Vec<Warning>, Error> {
    let Some(feed) = feed_name(&location.country_code) else {
        return Ok(Vec::new());
    };
    let url = format!("https://feeds.meteoalarm.org/feeds/meteoalarm-legacy-atom-{feed}");
    let body = client()?.get(url).send().await?.error_for_status()?.text().await?;
    Ok(for_place(parse_feed(&body)?, location))
}

/// Every warning in a country's feed (cancellations and green "no
/// warning" entries left out).
pub fn parse_feed(xml: &str) -> Result<Vec<Warning>, Error> {
    let doc = roxmltree::Document::parse(xml).map_err(|_| Error::MissingData)?;
    let text = |entry: roxmltree::Node, name: &str| -> Option<String> {
        entry.children().find(|n| n.tag_name().name() == name).and_then(|n| n.text()).map(|t| t.trim().to_string())
    };
    Ok(doc
        .descendants()
        .filter(|n| n.tag_name().name() == "entry")
        .filter_map(|entry| {
            if text(entry, "message_type").is_some_and(|t| t.eq_ignore_ascii_case("cancel"))
                || text(entry, "status").is_some_and(|t| !t.eq_ignore_ascii_case("actual"))
            {
                return None;
            }
            // "Orange Thunderstorm Warning issued for France - Isère"
            let title = text(entry, "title")?;
            let (level, kind) = level_and_kind(&title)?;
            Some(Warning {
                kind,
                level,
                area: text(entry, "areaDesc")?,
                onset: text(entry, "onset").or_else(|| text(entry, "effective")).and_then(|t| parse_rfc3339(&t))?,
                expires: text(entry, "expires").and_then(|t| parse_rfc3339(&t))?,
                id: text(entry, "identifier").unwrap_or_default(),
            })
        })
        .collect())
}

/// "Orange Thunderstorm Warning issued…" → (2, "thunderstorm").
fn level_and_kind(title: &str) -> Option<(u8, String)> {
    let mut words = title.split_whitespace();
    let level = match words.next()?.to_ascii_lowercase().as_str() {
        "yellow" => 1,
        "orange" => 2,
        "red" => 3,
        _ => return None,
    };
    let kind: Vec<&str> = words.take_while(|w| !w.eq_ignore_ascii_case("warning")).collect();
    (!kind.is_empty()).then(|| (level, kind.join(" ").to_lowercase()))
}

/// The warnings that apply to `location`, strongest first.
pub fn for_place(warnings: Vec<Warning>, location: &Location) -> Vec<Warning> {
    let names: Vec<String> = location
        .areas
        .iter()
        .chain(std::iter::once(&location.name))
        .map(|n| fold(n))
        .filter(|n| n.len() >= 3)
        .collect();
    let mut found: Vec<Warning> = warnings
        .into_iter()
        .filter(|w| {
            let area = format!(" {} ", fold(&w.area));
            names.iter().any(|n| area.contains(&format!(" {n} ")))
        })
        .collect();
    found.sort_by(|a, b| b.level.cmp(&a.level).then(a.onset.cmp(&b.onset)));
    // The same warning can be listed for several matching areas.
    found.dedup_by(|a, b| a.kind == b.kind && a.level == b.level && a.onset == b.onset && a.expires == b.expires);
    found
}

/// Lowercase, without accents or punctuation, single spaces: "Hautes-Alpes"
/// and "Hautes Alpes" compare equal.
fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        let plain = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'ç' | 'ć' | 'č' => "c",
            'ď' | 'đ' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => "i",
            'ł' | 'ľ' | 'ĺ' => "l",
            'ñ' | 'ń' | 'ň' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
            'ř' | 'ŕ' => "r",
            'ś' | 'š' | 'ş' | 'ș' => "s",
            'ť' | 'ţ' | 'ț' => "t",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
            'ý' | 'ÿ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            'ß' => "ss",
            'æ' => "ae",
            'œ' => "oe",
            c if c.is_alphanumeric() => {
                out.push(c);
                continue;
            }
            _ => " ",
        };
        out.push_str(plain);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fills in the country and areas of a place saved before they were
/// recorded: searches its name and keeps the result nearest to it (within
/// 30 km). Cached in `region-<place>.json`, so it's looked up once.
pub async fn with_areas(dir: Option<&Path>, location: &Location) -> Location {
    if !location.country_code.is_empty() {
        return location.clone();
    }
    let file = format!("region-{}.json", location.key());
    if let Some(cached) = dir
        .and_then(|d| std::fs::read(d.join(&file)).ok())
        .and_then(|b| serde_json::from_slice::<(String, Vec<String>)>(&b).ok())
    {
        return location.clone().with_areas(cached.0, cached.1);
    }
    let Ok(found) = crate::geocode::search_cities(&location.name, crate::Lang::En).await else {
        return location.clone();
    };
    let near = |l: &&Location| distance_km(l, location) < 30.0;
    let Some(best) =
        found.iter().filter(near).min_by(|a, b| distance_km(a, location).total_cmp(&distance_km(b, location)))
    else {
        return location.clone();
    };
    if let Some(dir) = dir
        && let Ok(json) = serde_json::to_vec(&(&best.country_code, &best.areas))
    {
        let _ = crate::cache::write_atomic(dir, &file, &json);
    }
    location.clone().with_areas(best.country_code.clone(), best.areas.clone())
}

fn distance_km(a: &Location, b: &Location) -> f64 {
    let (la1, la2) = (a.latitude.to_radians(), b.latitude.to_radians());
    let dla = la2 - la1;
    let dlo = (b.longitude - a.longitude).to_radians();
    let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    6371.0 * 2.0 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:cap="urn:oasis:names:tc:emergency:cap:1.2">
  <entry>
    <cap:areaDesc>Isère</cap:areaDesc>
    <cap:onset>2026-11-13T09:00:08+00:00</cap:onset>
    <cap:expires>2026-11-13T23:00:00+00:00</cap:expires>
    <cap:message_type>Alert</cap:message_type>
    <cap:status>Actual</cap:status>
    <cap:identifier>id-1</cap:identifier>
    <title>Yellow Avalanches Warning issued for France - Isère</title>
  </entry>
  <entry>
    <cap:areaDesc>Hautes Alpes</cap:areaDesc>
    <cap:onset>2026-11-13T15:00:00+01:00</cap:onset>
    <cap:expires>2026-11-14T06:00:00+01:00</cap:expires>
    <cap:message_type>Update</cap:message_type>
    <cap:status>Actual</cap:status>
    <cap:identifier>id-2</cap:identifier>
    <title>Orange Thunderstorm Warning issued for France - Hautes Alpes</title>
  </entry>
  <entry>
    <cap:areaDesc>Rhône</cap:areaDesc>
    <cap:onset>2026-11-13T09:00:00+00:00</cap:onset>
    <cap:expires>2026-11-13T23:00:00+00:00</cap:expires>
    <cap:message_type>Cancel</cap:message_type>
    <cap:status>Actual</cap:status>
    <title>Red Wind Warning issued for France - Rhône</title>
  </entry>
  <entry>
    <cap:areaDesc>Alb-Donau-Kreis und Stadt Ulm</cap:areaDesc>
    <cap:onset>2026-11-13T09:00:00Z</cap:onset>
    <cap:expires>2026-11-13T23:00:00Z</cap:expires>
    <cap:status>Actual</cap:status>
    <title>Red Rain-flood Warning issued for Germany - Alb-Donau-Kreis und Stadt Ulm</title>
  </entry>
</feed>"#;

    #[test]
    fn parses_levels_kinds_and_times() {
        let w = parse_feed(FEED).unwrap();
        assert_eq!(w.len(), 3, "the cancellation is left out");
        assert_eq!((w[0].level, w[0].kind.as_str(), w[0].area.as_str()), (1, "avalanches", "Isère"));
        assert_eq!((w[1].level, w[1].kind.as_str()), (2, "thunderstorm"));
        assert_eq!(w[1].onset, 1_794_578_400, "15:00 at UTC+1 is 14:00 UTC");
        assert_eq!((w[2].level, w[2].kind.as_str()), (3, "rain-flood"));
    }

    #[test]
    fn matches_places_by_area_or_name() {
        let all = parse_feed(FEED).unwrap();
        let grenoble = Location::new("Grenoble", "", 45.18, 5.71)
            .with_areas("FR", vec!["Auvergne-Rhône-Alpes".into(), "Isère".into(), "Grenoble".into()]);
        let w = for_place(all.clone(), &grenoble);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].kind, "avalanches");
        // Punctuation and accents don't matter.
        let gap = Location::new("Gap", "", 44.56, 6.08).with_areas("FR", vec!["Hautes-Alpes".into()]);
        assert_eq!(for_place(all.clone(), &gap)[0].level, 2);
        // A town named inside a longer area.
        let ulm = Location::new("Ulm", "", 48.4, 9.99).with_areas("DE", vec!["Baden-Württemberg".into()]);
        assert_eq!(for_place(all.clone(), &ulm)[0].level, 3);
        // No partial words: "Isère" doesn't match "Iser".
        let none =
            Location::new("Paris", "", 48.85, 2.35).with_areas("FR", vec!["Île-de-France".into(), "Paris".into()]);
        assert!(for_place(all, &none).is_empty());
    }

    #[test]
    fn feed_names() {
        assert_eq!(feed_name("fr"), Some("france"));
        assert_eq!(feed_name("US"), None);
    }

    #[test]
    #[ignore = "network: calls MeteoAlarm"]
    fn live_feed_parses() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let france = Location::new("Grenoble", "", 45.18, 5.71).with_areas("FR", vec!["Isère".into()]);
        rt.block_on(fetch_warnings(&france)).unwrap();
    }
}
