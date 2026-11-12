//! The system accent colour, so buttons and selections match the rest of
//! the device: Material You's wallpaper colours on Android 12+. The weather
//! sky stays as it is.

/// Colours as 0xRRGGBB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accent {
    /// For text, icons and outlines on the sky.
    pub light: u32,
    /// For filled buttons and highlights (drawn semi-transparent).
    pub strong: u32,
}

/// The app's own blue, when the system has no accent to offer.
pub const DEFAULT: Accent = Accent { light: 0x9E_CBFF, strong: 0x2F_80ED };

pub fn system() -> Accent {
    platform().unwrap_or(DEFAULT)
}

#[cfg(target_os = "android")]
fn platform() -> Option<Accent> {
    let app = crate::ANDROID_APP.get()?;
    let (light, strong) = crate::android::accent_colors(app)?;
    Some(Accent { light, strong })
}

#[cfg(not(target_os = "android"))]
fn platform() -> Option<Accent> {
    None
}
