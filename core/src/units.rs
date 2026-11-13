//! Metric or imperial: how temperatures, wind, rain and pressure are shown.
//! Data stays metric everywhere; only the formatting changes.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

impl Units {
    /// "11°" (or Fahrenheit), never "-0°".
    pub fn temp(self, celsius: f64) -> String {
        let t = match self {
            Units::Metric => celsius,
            Units::Imperial => celsius * 9.0 / 5.0 + 32.0,
        };
        let r = t.round();
        format!("{}°", if r == 0.0 { 0.0 } else { r })
    }

    /// A temperature difference ("+3°"), converted but not offset.
    pub fn temp_delta(self, celsius: f64) -> String {
        let d = match self {
            Units::Metric => celsius,
            Units::Imperial => celsius * 9.0 / 5.0,
        }
        .round();
        if d > 0.0 {
            format!("+{d}°")
        } else if d < 0.0 {
            format!("{d}°")
        } else {
            "0°".into()
        }
    }

    /// Speed and unit, separately: ("20", "km/h") or ("12", "mph").
    pub fn wind(self, kmh: f64) -> (String, &'static str) {
        match self {
            Units::Metric => (format!("{kmh:.0}"), "km/h"),
            Units::Imperial => (format!("{:.0}", kmh / 1.609_344), "mph"),
        }
    }

    pub fn wind_text(self, kmh: f64) -> String {
        let (v, u) = self.wind(kmh);
        format!("{v} {u}")
    }

    /// "0.4 mm" / "12 mm", or inches; empty when it rounds to nothing.
    /// Without the unit when `unit` is false (labels above chart bars).
    pub fn rain(self, mm: f64, unit: bool) -> String {
        let (value, label, min) = match self {
            Units::Metric => (mm, "mm", 0.05),
            Units::Imperial => (mm / 25.4, "in", 0.005),
        };
        if value < min {
            return String::new();
        }
        let n = match self {
            Units::Metric if value < 10.0 => format!("{value:.1}"),
            Units::Metric => format!("{value:.0}"),
            Units::Imperial => format!("{value:.2}"),
        };
        if unit { format!("{n} {label}") } else { n }
    }

    /// "1013 hPa" or "29.91 inHg".
    pub fn pressure(self, hpa: f64) -> String {
        match self {
            Units::Metric => format!("{hpa:.0} hPa"),
            Units::Imperial => format!("{:.2} inHg", hpa * 0.029_53),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts() {
        assert_eq!(Units::Metric.temp(-0.3), "0°");
        assert_eq!(Units::Imperial.temp(20.0), "68°");
        assert_eq!(Units::Imperial.temp_delta(5.0), "+9°");
        assert_eq!(Units::Imperial.wind(16.1), ("10".to_string(), "mph"));
        assert_eq!(Units::Metric.rain(0.36, true), "0.4 mm");
        assert_eq!(Units::Imperial.rain(25.4, true), "1.00 in");
        assert_eq!(Units::Imperial.rain(0.05, true), "");
        assert_eq!(Units::Imperial.pressure(1013.0), "29.91 inHg");
    }
}
