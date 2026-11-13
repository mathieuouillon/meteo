//! City search with Open-Meteo's geocoding API (free, no key).

use serde::Deserialize;

use crate::api::{Error, client};
use crate::i18n::Lang;
use crate::model::Location;

const MAX_RESULTS: usize = 8;

/// Finds places matching `query` (e.g. "Lyon"), most relevant first.
/// Queries shorter than 2 characters return nothing (the API needs 2).
/// Place names come back in `lang` ("Allemagne" rather than "Germany").
pub async fn search_cities(query: &str, lang: Lang) -> Result<Vec<Location>, Error> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let url = reqwest::Url::parse_with_params(
        "https://geocoding-api.open-meteo.com/v1/search",
        &[("name", query), ("count", &MAX_RESULTS.to_string()), ("language", lang.code()), ("format", "json")],
    )
    .expect("valid geocoding URL");
    let body = client()?.get(url).send().await?.error_for_status()?.bytes().await?;
    parse_places(&body)
}

/// Turns a raw geocoding response into locations.
pub fn parse_places(json: &[u8]) -> Result<Vec<Location>, Error> {
    let raw: RawSearch = serde_json::from_slice(json)?;
    Ok(raw
        .results
        .into_iter()
        .map(|p| {
            let detail = [p.admin1.clone(), p.country]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty() && *s != p.name)
                .collect::<Vec<_>>()
                .join(", ");
            let areas = [p.admin1, p.admin2, p.admin3, p.admin4].into_iter().flatten().collect();
            Location::new(p.name, detail, p.latitude, p.longitude).with_areas(p.country_code.unwrap_or_default(), areas)
        })
        .collect())
}

#[derive(Deserialize)]
struct RawSearch {
    /// Missing entirely when nothing matches.
    #[serde(default)]
    results: Vec<RawPlace>,
}

#[derive(Deserialize)]
struct RawPlace {
    name: String,
    latitude: f64,
    longitude: f64,
    admin1: Option<String>,
    admin2: Option<String>,
    admin3: Option<String>,
    admin4: Option<String>,
    country: Option<String>,
    country_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_results_and_builds_detail() {
        let json = br#"{"results": [
            {"name": "Lyon", "latitude": 45.75, "longitude": 4.85, "admin1": "Auvergne-Rhone-Alpes", "country": "France"},
            {"name": "Singapore", "latitude": 1.29, "longitude": 103.85, "admin1": "Singapore", "country": "Singapore"},
            {"name": "Nowhere", "latitude": 0.0, "longitude": 0.0}
        ]}"#;
        let places = parse_places(json).unwrap();
        assert_eq!(places[0].name, "Lyon");
        assert_eq!(places[0].detail, "Auvergne-Rhone-Alpes, France");
        assert_eq!(places[1].detail, "");
        assert_eq!(places[2].detail, "");
    }

    #[test]
    fn no_match_is_empty() {
        assert!(parse_places(br#"{"generationtime_ms": 0.1}"#).unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore = "hits the network"]
    async fn finds_lyon() {
        let places = search_cities("Lyon", Lang::En).await.unwrap();
        assert!(places.iter().any(|p| p.detail.ends_with("France")));
    }
}
