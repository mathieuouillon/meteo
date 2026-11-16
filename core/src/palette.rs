//! The UI's colours, in an editorial style: paper and ink. By day a paper
//! tone that follows the weather (warm in the sun, cool grey in the rain),
//! with dark ink and one accent; at night the same page inverted, ink-dark
//! with light text and a warm accent. The landscape is drawn tone on tone
//! in those colours.

use serde::Serialize;

use crate::condition::Condition;
use crate::landscape::{Phase, mix};

/// Colours as 0xRRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Palette {
    /// The page: a faint wash from the top to the paper.
    pub top: u32,
    pub bottom: u32,
    /// Panels on the page (pickers, the widget); the paper itself here.
    pub card: u32,
    /// A step toward the ink (pressed rows, tracks, chips).
    pub card_strong: u32,
    /// Main text and its quieter version.
    pub ink: u32,
    pub ink_soft: u32,
    /// The one colour: rain, links, the selected things.
    pub accent: u32,
    pub on_accent: u32,
    /// Rain/snow streaks and clouds drawn over the page.
    pub particle: u32,
    pub cloud: u32,
    /// True for the dark pages (night, storm).
    pub dark: bool,
}

/// A page from its paper, ink, quieter ink and accent.
const fn page(paper: u32, ink: u32, ink_soft: u32, accent: u32, dark: bool) -> Palette {
    Palette {
        top: paper,
        bottom: paper,
        card: paper,
        card_strong: paper,
        ink,
        ink_soft,
        accent,
        on_accent: paper,
        particle: ink_soft,
        cloud: paper,
        dark,
    }
}

impl Palette {
    pub fn for_weather(condition: Condition, phase: Phase) -> Self {
        use Condition::*;
        let night = phase == Phase::Night;
        let twilight = matches!(phase, Phase::Dawn | Phase::Dusk);
        let mut p = match (condition, night) {
            (Thunderstorm, _) => page(0x17181C, 0xEDE9E2, 0x8F9197, 0xF0C24B, true),
            (Fog, true) => page(0x1A1C20, 0xE7E5E0, 0x8E9096, 0xBFC6CF, true),
            (Clear | PartlyCloudy, true) => page(0x12151D, 0xEDE6DA, 0x8E909A, 0xE7B15A, true),
            (_, true) => page(0x15181E, 0xE6E2DA, 0x8D9097, 0x9DB8D9, true),
            (Fog, false) => page(0xE9E8E5, 0x23252A, 0x72757A, 0x6E7887, false),
            (Clear, false) if twilight => page(0xF4E6DC, 0x2A1C18, 0x80665A, 0xC24E36, false),
            (Clear, false) => page(0xF5EFE4, 0x1E1A15, 0x7A6F60, 0xC0581F, false),
            (PartlyCloudy, false) => page(0xF2EEE7, 0x1E1D1A, 0x77716A, 0xB8682A, false),
            (Cloudy, false) => page(0xEEEDEA, 0x1C1D20, 0x6F7277, 0x4F6A86, false),
            (Drizzle | Rain, false) => page(0xEDEFEE, 0x15191F, 0x68707A, 0x2F5D8A, false),
            (Snow, false) => page(0xF5F6F6, 0x18202A, 0x6A7480, 0x4C7DAF, false),
        };
        // A faint wash at the top of the page, toward white by day and black
        // at night.
        p.top = if p.dark { mix(p.bottom, 0x000000, 0.25) } else { mix(p.bottom, 0xFFFFFF, 0.5) };
        p.card_strong = mix(p.card, p.ink, if p.dark { 0.10 } else { 0.07 });
        p.cloud = mix(p.bottom, p.ink, if p.dark { 0.08 } else { 0.06 });
        p
    }

    /// The landscape, far to near, tone on tone: by day the paper with more
    /// and more ink (a touch of the accent in it), so the far ridge is the
    /// palest; at night the far ridge is the lightest against the dark page.
    pub fn hills(&self) -> [u32; 4] {
        let tint = mix(self.bottom, self.accent, 0.12);
        if self.dark {
            [0.16, 0.11, 0.07, 0.035].map(|t| mix(tint, self.ink, t))
        } else {
            [0.13, 0.22, 0.34, 0.48].map(|t| mix(tint, self.ink, t))
        }
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
