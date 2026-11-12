//! The UI's colours follow the weather: warm amber on a clear afternoon,
//! cool blue-white in the rain, charcoal in a storm, deep blue at night.
//! One palette per condition and time of day; the UI binds its roles.

use serde::Serialize;

use crate::condition::Condition;
use crate::landscape::{Phase, mix};

/// Colours as 0xRRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Palette {
    /// Background gradient.
    pub top: u32,
    pub bottom: u32,
    /// Cards on top of it.
    pub card: u32,
    /// A slightly stronger card tone (chips, tracks, pressed rows).
    pub card_strong: u32,
    /// Main text and its quieter version.
    pub ink: u32,
    pub ink_soft: u32,
    /// Accent for highlights, the curve, buttons.
    pub accent: u32,
    pub on_accent: u32,
    /// Rain/snow streaks and clouds drawn over the background.
    pub particle: u32,
    pub cloud: u32,
    /// True for the dark palettes (night, storm, fog).
    pub dark: bool,
}

const fn light(top: u32, bottom: u32, card: u32, ink: u32, ink_soft: u32, accent: u32) -> Palette {
    Palette {
        top,
        bottom,
        card,
        card_strong: card,
        ink,
        ink_soft,
        accent,
        on_accent: 0xFFFFFF,
        particle: accent,
        cloud: 0xFFFFFF,
        dark: false,
    }
}

const fn dark(top: u32, bottom: u32, card: u32, ink: u32, ink_soft: u32, accent: u32) -> Palette {
    Palette {
        top,
        bottom,
        card,
        card_strong: card,
        ink,
        ink_soft,
        accent,
        on_accent: 0x10131A,
        particle: 0xFFFFFF,
        cloud: 0x5E6675,
        dark: true,
    }
}

impl Palette {
    pub fn for_weather(condition: Condition, phase: Phase) -> Self {
        use Condition::*;
        let night = phase == Phase::Night;
        let twilight = matches!(phase, Phase::Dawn | Phase::Dusk);
        let mut p = match (condition, night) {
            (Thunderstorm, _) => dark(0x3B4049, 0x1E2227, 0x464C57, 0xF1F3F6, 0xB9C0CA, 0xF2C14E),
            (Fog, false) => light(0xE3E4E6, 0xC9CBCF, 0xF2F3F4, 0x24272C, 0x5F646B, 0x7D8590),
            (Fog, true) => dark(0x3A3E44, 0x23262B, 0x474C53, 0xEEF0F2, 0xB4B9C0, 0xC9CED6),
            (Clear | PartlyCloudy, true) => dark(0x1E2747, 0x0E1430, 0x2A3460, 0xEAF0FB, 0xA9B6D6, 0xBFD2FF),
            (_, true) => dark(0x232B3A, 0x111620, 0x2F3849, 0xE8EDF5, 0xA8B2C2, 0x9CC3FF),
            (Clear, false) if twilight => light(0xF8DCC6, 0xEDB4A2, 0xFFF1E8, 0x40231A, 0x8A5E50, 0xE0704F),
            (Clear, false) => light(0xF8E9D2, 0xF2D3A6, 0xFFF7EC, 0x3B2A12, 0x7D6446, 0xE8902A),
            (PartlyCloudy, false) => light(0xF1ECE4, 0xDCE3EC, 0xFFFFFF, 0x262C35, 0x66707D, 0xE59A3A),
            (Cloudy, false) => light(0xE6E9EE, 0xCDD4DD, 0xF6F8FA, 0x222831, 0x5D6672, 0x5E7896),
            (Drizzle | Rain, false) => light(0xE4EDF8, 0xC7D7EC, 0xF7FAFE, 0x172436, 0x56677F, 0x3D7BD9),
            (Snow, false) => light(0xF3F7FB, 0xDCE6F1, 0xFFFFFF, 0x1C2836, 0x5D6B7C, 0x5E9AD6),
        };
        // Chips and tracks: a step from the card toward the ink.
        p.card_strong = mix(p.card, p.ink, if p.dark { 0.10 } else { 0.07 });
        if !p.dark {
            // Streaks a little softer than the accent, clouds a touch tinted.
            p.particle = mix(p.accent, p.bottom, 0.25);
            p.cloud = mix(0xFFFFFF, p.bottom, 0.15);
        }
        p
    }

    /// Tone-on-tone hills for the landscape, far to near: the background
    /// colour with more and more ink, so the scene stays quiet.
    pub fn hills(&self) -> [u32; 4] {
        let amounts = if self.dark { [0.05, 0.09, 0.13, 0.18] } else { [0.06, 0.10, 0.15, 0.21] };
        amounts.map(|t| mix(self.bottom, self.ink, t))
    }
}

/// Which weather icon to draw (the UI draws them; see `WeatherIcon` in
/// main.slint).
pub fn icon_kind(condition: Condition, is_day: bool) -> &'static str {
    use Condition::*;
    match (condition, is_day) {
        (Clear, true) => "clear-day",
        (Clear, false) => "clear-night",
        (PartlyCloudy, true) => "partly-day",
        (PartlyCloudy, false) => "partly-night",
        (Cloudy, _) => "cloudy",
        (Fog, _) => "fog",
        (Drizzle, _) => "drizzle",
        (Rain, _) => "rain",
        (Snow, _) => "snow",
        (Thunderstorm, _) => "storm",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_follow_weather_and_time() {
        let sunny = Palette::for_weather(Condition::Clear, Phase::Day);
        let night = Palette::for_weather(Condition::Clear, Phase::Night);
        let storm = Palette::for_weather(Condition::Thunderstorm, Phase::Day);
        assert!(!sunny.dark && night.dark && storm.dark);
        assert_ne!(sunny.top, Palette::for_weather(Condition::Rain, Phase::Day).top);
        assert_ne!(sunny.top, Palette::for_weather(Condition::Clear, Phase::Dusk).top);
        // Hills get darker (closer to the ink) from far to near on a light palette.
        let h = sunny.hills();
        assert!(h.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn icons() {
        assert_eq!(icon_kind(Condition::Clear, false), "clear-night");
        assert_eq!(icon_kind(Condition::Thunderstorm, true), "storm");
    }
}
