//! Which weather model the forecast comes from, and the user's settings.
//!
//! All sources go through Open-Meteo (no API keys); they differ by the
//! national weather service's model behind them. Some cover fewer days or
//! have no rain probability; parsing copes with that.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::units::Units;

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

    pub fn name(self, lang: Lang) -> &'static str {
        let fr = lang == Lang::Fr;
        match self {
            Source::BestMatch => {
                if fr {
                    "Meilleur choix"
                } else {
                    "Best match"
                }
            }
            Source::MeteoFrance => "Météo-France",
            Source::Ecmwf => "ECMWF",
            Source::Dwd => {
                if fr {
                    "DWD (Allemagne)"
                } else {
                    "DWD (Germany)"
                }
            }
            Source::Noaa => {
                if fr {
                    "NOAA (États-Unis)"
                } else {
                    "NOAA (USA)"
                }
            }
            Source::UkMetOffice => {
                if fr {
                    "Met Office (Royaume-Uni)"
                } else {
                    "UK Met Office"
                }
            }
            Source::MetNorway => {
                if fr {
                    "MET Norvège"
                } else {
                    "MET Norway"
                }
            }
            Source::Knmi => {
                if fr {
                    "KNMI (Pays-Bas)"
                } else {
                    "KNMI (Netherlands)"
                }
            }
            Source::Dmi => {
                if fr {
                    "DMI (Danemark)"
                } else {
                    "DMI (Denmark)"
                }
            }
            Source::Canada => {
                if fr {
                    "Environnement Canada"
                } else {
                    "Environment Canada"
                }
            }
            Source::Jma => {
                if fr {
                    "JMA (Japon)"
                } else {
                    "JMA (Japan)"
                }
            }
        }
    }

    pub fn description(self, lang: Lang) -> &'static str {
        let fr = lang == Lang::Fr;
        match self {
            Source::BestMatch => {
                if fr {
                    "Open-Meteo combine les meilleurs modèles pour le lieu"
                } else {
                    "Open-Meteo combines the best models for the place"
                }
            }
            Source::MeteoFrance => {
                if fr {
                    "AROME et ARPEGE, très détaillés sur la France · environ 4 jours, sans % de pluie"
                } else {
                    "AROME & ARPEGE, very detailed over France · about 4 days, no rain %"
                }
            }
            Source::Ecmwf => {
                if fr {
                    "IFS, le modèle global européen"
                } else {
                    "IFS, the European global model"
                }
            }
            Source::Dwd => {
                if fr {
                    "ICON, détaillé sur l’Europe centrale"
                } else {
                    "ICON, detailed over Central Europe"
                }
            }
            Source::Noaa => {
                if fr {
                    "GFS et HRRR, global, détaillé sur les États-Unis"
                } else {
                    "GFS & HRRR, global, detailed over the US"
                }
            }
            Source::UkMetOffice => {
                if fr {
                    "UKV et global, détaillé sur le Royaume-Uni"
                } else {
                    "UKV & global, detailed over the UK"
                }
            }
            Source::MetNorway => {
                if fr {
                    "MET Nordic, détaillé sur la Scandinavie"
                } else {
                    "MET Nordic, detailed over Scandinavia"
                }
            }
            Source::Knmi => {
                if fr {
                    "HARMONIE, détaillé sur les Pays-Bas"
                } else {
                    "HARMONIE, detailed over the Netherlands"
                }
            }
            Source::Dmi => {
                if fr {
                    "HARMONIE, détaillé sur le Danemark et l’Europe du Nord"
                } else {
                    "HARMONIE, detailed over Denmark & Northern Europe"
                }
            }
            Source::Canada => {
                if fr {
                    "GEM, détaillé sur le Canada"
                } else {
                    "GEM, detailed over Canada"
                }
            }
            Source::Jma => {
                if fr {
                    "MSM et GSM, détaillé sur le Japon · sans % de pluie"
                } else {
                    "MSM & GSM, detailed over Japan · no rain %"
                }
            }
        }
    }
}

/// User preferences, saved as `settings.json` next to the cities.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub units: Units,
    #[serde(default)]
    pub colors: ColorMode,
    #[serde(default)]
    pub language: LanguageChoice,
    /// Notifications: a summary each morning, and a warning when rain is
    /// about to start. Off until the user turns them on.
    #[serde(default)]
    pub morning_summary: bool,
    #[serde(default)]
    pub rain_alerts: bool,
    /// Orange and red official weather warnings (MeteoAlarm).
    #[serde(default)]
    pub warning_alerts: bool,
}

/// Where the UI's colours come from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    /// From the weather (palette.rs).
    #[default]
    Weather,
    /// Material You, from the wallpaper (Android 12+).
    Wallpaper,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageChoice {
    /// Follow the phone.
    #[default]
    System,
    English,
    French,
}

impl Settings {
    /// The language to use, given the phone's.
    pub fn lang(&self, system: Lang) -> Lang {
        match self.language {
            LanguageChoice::System => system,
            LanguageChoice::English => Lang::En,
            LanguageChoice::French => Lang::Fr,
        }
    }
}

const FILE_NAME: &str = "settings.json";

impl Settings {
    /// Defaults if the file is missing or unreadable.
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join(FILE_NAME)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
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
        Settings { source: Source::MeteoFrance, ..Settings::default() }.save(&dir).unwrap();
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
