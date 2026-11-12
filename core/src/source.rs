//! Which weather model the forecast comes from, and the user's settings.
//!
//! All sources go through Open-Meteo (no API keys); they differ by the
//! national weather service's model behind them. Some cover fewer days or
//! have no rain probability; parsing copes with that.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    BestMatch,
    MeteoFrance,
    Ecmwf,
    Dwd,
    Noaa,
    UkMetOffice,
    MetNorway,
    Knmi,
    Dmi,
    Canada,
    Jma,
}

impl Source {
    pub const ALL: [Source; 11] = [
        Source::BestMatch,
        Source::MeteoFrance,
        Source::Ecmwf,
        Source::Dwd,
        Source::Noaa,
        Source::UkMetOffice,
        Source::MetNorway,
        Source::Knmi,
        Source::Dmi,
        Source::Canada,
        Source::Jma,
    ];

    /// Open-Meteo's `models=` value.
    pub fn model_id(self) -> &'static str {
        match self {
            Source::BestMatch => "best_match",
            Source::MeteoFrance => "meteofrance_seamless",
            Source::Ecmwf => "ecmwf_ifs025",
            Source::Dwd => "icon_seamless",
            Source::Noaa => "gfs_seamless",
            Source::UkMetOffice => "ukmo_seamless",
            Source::MetNorway => "metno_seamless",
            Source::Knmi => "knmi_seamless",
            Source::Dmi => "dmi_seamless",
            Source::Canada => "gem_seamless",
            Source::Jma => "jma_seamless",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::BestMatch => "Best match",
            Source::MeteoFrance => "Météo-France",
            Source::Ecmwf => "ECMWF",
            Source::Dwd => "DWD (Germany)",
            Source::Noaa => "NOAA (USA)",
            Source::UkMetOffice => "UK Met Office",
            Source::MetNorway => "MET Norway",
            Source::Knmi => "KNMI (Netherlands)",
            Source::Dmi => "DMI (Denmark)",
            Source::Canada => "Environment Canada",
            Source::Jma => "JMA (Japan)",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Source::BestMatch => "Open-Meteo combines the best models for the place",
            Source::MeteoFrance => "AROME & ARPEGE, very detailed over France · about 4 days, no rain %",
            Source::Ecmwf => "IFS, the European global model",
            Source::Dwd => "ICON, detailed over Central Europe",
            Source::Noaa => "GFS & HRRR, global, detailed over the US",
            Source::UkMetOffice => "UKV & global, detailed over the UK",
            Source::MetNorway => "MET Nordic, detailed over Scandinavia",
            Source::Knmi => "HARMONIE, detailed over the Netherlands",
            Source::Dmi => "HARMONIE, detailed over Denmark & Northern Europe",
            Source::Canada => "GEM, detailed over Canada",
            Source::Jma => "MSM & GSM, detailed over Japan · no rain %",
        }
    }
}

/// User preferences, saved as `settings.json` next to the cities.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub source: Source,
}

const FILE_NAME: &str = "settings.json";

impl Settings {
    /// Defaults if the file is missing or unreadable.
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE_NAME))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> io::Result<()> {
        crate::cache::write_atomic(dir, FILE_NAME, &serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_default() {
        let dir = std::env::temp_dir().join(format!("meteo-settings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Settings::load(&dir).source, Source::BestMatch);
        Settings { source: Source::MeteoFrance }.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir).source, Source::MeteoFrance);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn every_source_has_a_distinct_model() {
        let mut ids: Vec<_> = Source::ALL.iter().map(|s| s.model_id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), Source::ALL.len());
    }
}
