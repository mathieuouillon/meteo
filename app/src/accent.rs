//! The system accent colour, so buttons and selections match the rest of
//! the device: Material You's wallpaper colours on Android 12+, the accent
//! chosen in System Settings on macOS. The weather sky stays as it is.

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

#[cfg(target_os = "macos")]
fn platform() -> Option<Accent> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    let color = NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let channel = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() as u32;
    let strong = channel(color.redComponent()) << 16 | channel(color.greenComponent()) << 8 | channel(color.blueComponent());
    // macOS gives one colour; lighten it for use on the dark sky.
    Some(Accent { light: mix(strong, 0xFF_FFFF, 0.45), strong })
}

#[cfg(not(any(target_os = "android", target_os = "macos")))]
fn platform() -> Option<Accent> {
    None
}

/// Blends `a` toward `b` by `t` (0–1), per channel.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mix(a: u32, b: u32, t: f64) -> u32 {
    let ch = |c: u32, shift: u32| ((c >> shift) & 0xFF) as f64;
    [16, 8, 0].iter().fold(0, |acc, &s| {
        let v = ch(a, s) + (ch(b, s) - ch(a, s)) * t;
        acc | ((v.round() as u32) << s)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_channels() {
        assert_eq!(mix(0x00_0000, 0xFF_FFFF, 0.5), 0x80_8080);
        assert_eq!(mix(0x2F_80ED, 0xFF_FFFF, 0.0), 0x2F_80ED);
        assert_eq!(mix(0x2F_80ED, 0xFF_FFFF, 1.0), 0xFF_FFFF);
    }
}
