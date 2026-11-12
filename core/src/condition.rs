use serde::{Deserialize, Serialize};

/// Weather condition, collapsed from WMO weather codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Condition {
    Clear,
    PartlyCloudy,
    Cloudy,
    Fog,
    Drizzle,
    Rain,
    Snow,
    Thunderstorm,
}

impl Condition {
    /// Maps a WMO weather code (as used by Open-Meteo) to a condition.
    pub fn from_wmo(code: u8) -> Self {
        match code {
            0 | 1 => Self::Clear,
            2 => Self::PartlyCloudy,
            3 => Self::Cloudy,
            45 | 48 => Self::Fog,
            51..=57 => Self::Drizzle,
            61..=67 | 80..=82 => Self::Rain,
            71..=77 | 85 | 86 => Self::Snow,
            95..=99 => Self::Thunderstorm,
            _ => Self::Cloudy,
        }
    }

    pub fn icon(self, is_day: bool) -> &'static str {
        match (self, is_day) {
            (Self::Clear, true) => "☀️",
            (Self::Clear, false) => "🌙",
            (Self::PartlyCloudy, true) => "⛅",
            (Self::PartlyCloudy, false) => "☁️",
            (Self::Cloudy, _) => "☁️",
            (Self::Fog, _) => "🌫️",
            (Self::Drizzle, _) => "🌦️",
            (Self::Rain, _) => "🌧️",
            (Self::Snow, _) => "❄️",
            (Self::Thunderstorm, _) => "⛈️",
        }
    }

    /// Background mood for this condition at this time of day.
    pub fn sky(self, is_day: bool) -> Sky {
        let (top, bottom) = match (self, is_day) {
            (_, false) => match self {
                Self::Clear | Self::PartlyCloudy => (0x0B1026, 0x2B3A67),
                _ => (0x1C1F2A, 0x3A4050),
            },
            (Self::Clear, true) => (0x2F80ED, 0x8EC5FC),
            (Self::PartlyCloudy, true) => (0x4A7FBF, 0xA9C6E8),
            (Self::Cloudy | Self::Fog, true) => (0x6B7B8C, 0xB8C2CC),
            (Self::Drizzle | Self::Rain, true) => (0x3E5068, 0x7F93A8),
            (Self::Snow, true) => (0x8FA6BF, 0xE3ECF5),
            (Self::Thunderstorm, true) => (0x2C2F3E, 0x5B5F75),
        };
        Sky { top, bottom }
    }
}

/// A vertical background gradient, as 0xRRGGBB colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sky {
    pub top: u32,
    pub bottom: u32,
}

impl Sky {
    pub fn top_css(self) -> String {
        format!("#{:06x}", self.top)
    }

    pub fn bottom_css(self) -> String {
        format!("#{:06x}", self.bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wmo_codes_map_to_conditions() {
        assert_eq!(Condition::from_wmo(0), Condition::Clear);
        assert_eq!(Condition::from_wmo(3), Condition::Cloudy);
        assert_eq!(Condition::from_wmo(63), Condition::Rain);
        assert_eq!(Condition::from_wmo(81), Condition::Rain);
        assert_eq!(Condition::from_wmo(75), Condition::Snow);
        assert_eq!(Condition::from_wmo(96), Condition::Thunderstorm);
    }
}
