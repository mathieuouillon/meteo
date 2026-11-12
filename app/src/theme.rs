//! Material 3 colour roles for the UI, from the system's Material You
//! palettes (Android 12+, taken from the wallpaper) in light or dark mode.
//! Without dynamic colours (older Android, desktop preview) a built-in blue
//! scheme is used. The nature scene keeps its own weather colours.

/// Colours as 0xRRGGBB, named after Material 3's roles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheme {
    pub primary: u32,
    pub on_primary: u32,
    pub primary_container: u32,
    pub on_primary_container: u32,
    pub secondary_container: u32,
    pub on_secondary_container: u32,
    pub tertiary_container: u32,
    pub on_tertiary_container: u32,
    pub surface: u32,
    pub surface_container: u32,
    pub surface_container_high: u32,
    pub on_surface: u32,
    pub on_surface_variant: u32,
    pub outline_variant: u32,
}

/// Material 3 baseline-style blue, light.
const LIGHT: Scheme = Scheme {
    primary: 0x3C5FA8,
    on_primary: 0xFFFFFF,
    primary_container: 0xD9E2FF,
    on_primary_container: 0x001944,
    secondary_container: 0xDCE2F9,
    on_secondary_container: 0x141B2C,
    tertiary_container: 0xFBD7FC,
    on_tertiary_container: 0x29132D,
    surface: 0xFAF9FF,
    surface_container: 0xEDEDF4,
    surface_container_high: 0xE3E4EC,
    on_surface: 0x1A1B21,
    on_surface_variant: 0x44464F,
    outline_variant: 0xC5C6D0,
};

/// Material 3 baseline-style blue, dark.
const DARK: Scheme = Scheme {
    primary: 0xB0C6FF,
    on_primary: 0x002D6E,
    primary_container: 0x22468F,
    on_primary_container: 0xD9E2FF,
    secondary_container: 0x3F4759,
    on_secondary_container: 0xDCE2F9,
    tertiary_container: 0x583E5B,
    on_tertiary_container: 0xFBD7FC,
    surface: 0x121318,
    surface_container: 0x1E1F25,
    surface_container_high: 0x292A2F,
    on_surface: 0xE3E2E9,
    on_surface_variant: 0xC5C6D0,
    outline_variant: 0x44464F,
};

pub fn system() -> Scheme {
    platform().unwrap_or_else(|| if preview_dark() { DARK } else { LIGHT })
}

/// `METEO_DARK=1 cargo run -p meteo` previews the dark scheme on desktop.
fn preview_dark() -> bool {
    std::env::var_os("METEO_DARK").is_some()
}

#[cfg(target_os = "android")]
fn platform() -> Option<Scheme> {
    let app = crate::ANDROID_APP.get()?;
    let sys = crate::android::system_colors(app)?;
    let t = |p: &str, n: u32| sys.tone(p, n);
    // Material 3's mapping from tonal palettes to roles.
    let dynamic = if sys.dark {
        Some(Scheme {
            primary: t("accent1", 200)?,
            on_primary: t("accent1", 800)?,
            primary_container: t("accent1", 700)?,
            on_primary_container: t("accent1", 100)?,
            secondary_container: t("accent2", 700)?,
            on_secondary_container: t("accent2", 100)?,
            tertiary_container: t("accent3", 700)?,
            on_tertiary_container: t("accent3", 100)?,
            surface: t("neutral1", 900)?,
            surface_container: mix(t("neutral1", 900)?, t("neutral1", 800)?, 0.25),
            surface_container_high: mix(t("neutral1", 900)?, t("neutral1", 800)?, 0.7),
            on_surface: t("neutral1", 100)?,
            on_surface_variant: t("neutral2", 200)?,
            outline_variant: t("neutral2", 700)?,
        })
    } else {
        Some(Scheme {
            primary: t("accent1", 600)?,
            on_primary: t("accent1", 0)?,
            primary_container: t("accent1", 100)?,
            on_primary_container: t("accent1", 900)?,
            secondary_container: t("accent2", 100)?,
            on_secondary_container: t("accent2", 900)?,
            tertiary_container: t("accent3", 100)?,
            on_tertiary_container: t("accent3", 900)?,
            surface: t("neutral1", 10)?,
            surface_container: t("neutral2", 50)?,
            surface_container_high: t("neutral2", 100)?,
            on_surface: t("neutral1", 900)?,
            on_surface_variant: t("neutral2", 700)?,
            outline_variant: t("neutral2", 200)?,
        })
    };
    Some(dynamic.unwrap_or(if sys.dark { DARK } else { LIGHT }))
}

#[cfg(not(target_os = "android"))]
fn platform() -> Option<Scheme> {
    None
}

/// Blends `a` toward `b` by `t`, per channel.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn mix(a: u32, b: u32, t: f64) -> u32 {
    [16, 8, 0].iter().fold(0, |acc, &s| {
        let (x, y) = (((a >> s) & 0xFF) as f64, ((b >> s) & 0xFF) as f64);
        acc | (((x + (y - x) * t).round() as u32) << s)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_channels() {
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
        assert_eq!(mix(0x123456, 0xFFFFFF, 0.0), 0x123456);
    }
}
