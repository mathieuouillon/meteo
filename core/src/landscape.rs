//! The nature scene behind the forecast: a sky, the sun or moon, and layers
//! of mountains and hills with a pine treeline, coloured for the time of day
//! and the weather.
//!
//! Everything is computed here as plain data — path strings in a fixed
//! 400×300 viewbox and colours as 0xRRGGBB — so the UI only draws it. Each
//! city gets its own skyline (the shapes are seeded from its name).

use std::f64::consts::PI;
use std::fmt::Write;

use serde::Serialize;

use crate::condition::Condition;

/// Size of the coordinate box the layer paths are drawn in.
pub const LANDSCAPE_WIDTH: f64 = 400.0;
pub const LANDSCAPE_HEIGHT: f64 = 300.0;

/// Part of the day, from the sun's position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Phase {
    Night,
    Dawn,
    Day,
    Dusk,
}

/// Minutes around sunrise and sunset that count as dawn and dusk.
const TWILIGHT_MIN: i64 = 45;

impl Phase {
    /// From local times as "2026-11-12T07:42" (only the clock part is used).
    /// Without sunrise/sunset (old cache), falls back to day/night.
    pub fn at(now: &str, sunrise: Option<&str>, sunset: Option<&str>, is_day: bool) -> Self {
        let (Some(now), Some(rise), Some(set)) = (minutes(now), sunrise.and_then(minutes), sunset.and_then(minutes))
        else {
            return if is_day { Phase::Day } else { Phase::Night };
        };
        if (now - rise).abs() <= TWILIGHT_MIN {
            Phase::Dawn
        } else if (now - set).abs() <= TWILIGHT_MIN {
            Phase::Dusk
        } else if now > rise && now < set {
            Phase::Day
        } else {
            Phase::Night
        }
    }
}

/// Minutes since midnight in "…THH:MM".
fn minutes(t: &str) -> Option<i64> {
    let h: i64 = t.get(11..13)?.parse().ok()?;
    let m: i64 = t.get(14..16)?.parse().ok()?;
    Some(h * 60 + m)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Landscape {
    pub phase: Phase,
    pub sky_top: u32,
    pub sky_bottom: u32,
    /// Sun or moon, as fractions of the landscape box (0–1); hidden when
    /// clouds cover it.
    pub orb_visible: bool,
    pub orb_is_moon: bool,
    pub orb_x: f64,
    pub orb_y: f64,
    pub orb_color: u32,
    /// Far to near.
    pub layers: Vec<LandscapeLayer>,
    /// The nearest layer's colour, to fill the screen below the landscape.
    pub ground: u32,
    /// True when the sky is light enough that text on it should be dark.
    pub light_sky: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LandscapeLayer {
    /// Closed path in the 400×300 box.
    pub path: String,
    pub color: u32,
}

/// What the current weather does to the scene.
struct Look {
    /// 0 = clear, 1 = fully overcast.
    overcast: f64,
    snow: bool,
    fog: bool,
}

fn look(c: Condition) -> Look {
    use Condition::*;
    let (overcast, snow, fog) = match c {
        Clear => (0.0, false, false),
        PartlyCloudy => (0.25, false, false),
        Cloudy => (0.7, false, false),
        Fog => (0.6, false, true),
        Drizzle => (0.65, false, false),
        Rain => (0.85, false, false),
        Snow => (0.7, true, false),
        Thunderstorm => (1.0, false, false),
    };
    Look { overcast, snow, fog }
}

/// Clear-sky colours (top, bottom) and land colours (far → near) per phase.
fn palette(phase: Phase) -> ((u32, u32), [u32; 4]) {
    match phase {
        Phase::Day => ((0x3A7BD5, 0xA8D8F0), [0x7FA3C4, 0x5E8F73, 0x3F6E50, 0x2A4F39]),
        Phase::Dawn => ((0x4B5C9E, 0xF6B48A), [0x9A86B0, 0x6E6496, 0x4A4573, 0x2E2C50]),
        Phase::Dusk => ((0x34386F, 0xF28C6B), [0x8C6E9E, 0x5E5486, 0x3E3963, 0x272646]),
        Phase::Night => ((0x0B1026, 0x2B3A67), [0x2A3A60, 0x1E2A4A, 0x151E37, 0x0E1426]),
    }
}

/// Builds the scene. `seed` picks the skyline (use the city name);
/// `sun_progress` is how far the sun is between sunrise (0) and sunset (1).
pub fn landscape(condition: Condition, phase: Phase, sun_progress: f64, seed: u64) -> Landscape {
    let look = look(condition);
    let ((top, bottom), land) = palette(phase);
    let night = phase == Phase::Night;

    // Overcast skies turn grey; fog washes the sky towards its horizon colour.
    let grey = if night { (0x1E222B, 0x363C48) } else { (0x7D8996, 0xC4CBD3) };
    let mut sky_top = mix(top, grey.0, look.overcast);
    let mut sky_bottom = mix(bottom, grey.1, look.overcast);
    if look.fog {
        sky_top = mix(sky_top, sky_bottom, 0.5);
    }
    if look.snow {
        sky_bottom = mix(sky_bottom, 0xE6ECF2, if night { 0.15 } else { 0.4 });
    }

    // Atmospheric perspective: farther layers fade into the horizon colour,
    // much more in fog. Snow whitens the land, cloud cover mutes it.
    let haze = if look.fog { [0.75, 0.6, 0.4, 0.2] } else { [0.45, 0.25, 0.1, 0.0] };
    let snow_tone = if night { 0x5A6684 } else { 0xE9EEF3 };
    let colors: Vec<u32> = land
        .iter()
        .zip(haze)
        .enumerate()
        .map(|(i, (&c, h))| {
            let mut c = mix(c, desaturate(c), look.overcast * 0.6);
            if look.snow {
                c = mix(c, snow_tone, [0.85, 0.7, 0.45, 0.3][i]);
            }
            mix(c, sky_bottom, h)
        })
        .collect();

    let mut rng = Rng::new(seed);
    let paths = [
        ridge(&mut rng, 85.0, 70.0, 0.62),  // far mountains
        ridge(&mut rng, 150.0, 45.0, 0.5),  // nearer mountains
        hills(&mut rng, 200.0, 22.0, true),        // hills with pines
        hills(&mut rng, 245.0, 14.0, true),        // foreground with pines
    ];

    let (orb_x, orb_y, orb_color, orb_is_moon) = if night {
        (0.76, 0.16, 0xF2F4FF, true)
    } else {
        let p = sun_progress.clamp(0.0, 1.0);
        let low = matches!(phase, Phase::Dawn | Phase::Dusk);
        // Along an arc: low at sunrise and sunset, high at midday.
        (0.12 + 0.76 * p, 0.42 - 0.32 * (PI * p).sin(), if low { 0xFFB46B } else { 0xFFF1C2 }, false)
    };

    let light_sky = !night && look.overcast < 0.9 && luminance(sky_top) > 0.42;
    Landscape {
        phase,
        sky_top,
        sky_bottom,
        orb_visible: look.overcast < 0.6 && !look.fog,
        orb_is_moon,
        orb_x,
        orb_y,
        orb_color,
        ground: *colors.last().expect("four layers"),
        layers: paths.into_iter().zip(colors).map(|(path, color)| LandscapeLayer { path, color }).collect(),
        light_sky,
    }
}

/// A seed from a name (FNV-1a), so each city keeps its skyline.
pub fn seed_for(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3))
}

/// A jagged mountain ridge by midpoint displacement, closed at the bottom.
/// `base`: average height of the ridge (y), `amp`: how tall the peaks get,
/// `rough`: how much each subdivision keeps of the previous amplitude.
fn ridge(rng: &mut Rng, base: f64, amp: f64, rough: f64) -> String {
    let n = 64; // power of two
    let mut ys = vec![0.0; n + 1];
    ys[0] = base + rng.range(-amp, amp) * 0.5;
    ys[n] = base + rng.range(-amp, amp) * 0.5;
    let (mut step, mut a) = (n, amp);
    while step > 1 {
        let half = step / 2;
        for i in (half..n).step_by(step) {
            ys[i] = (ys[i - half] + ys[i + half]) / 2.0 + rng.range(-a, a);
        }
        step = half;
        a *= rough;
    }
    let points: Vec<(f64, f64)> = ys
        .iter()
        .enumerate()
        .map(|(i, &y)| (i as f64 * LANDSCAPE_WIDTH / n as f64, y.clamp(10.0, LANDSCAPE_HEIGHT)))
        .collect();
    closed_path(&points)
}

/// Rolling hills (a sum of gentle waves), optionally with pines on top.
fn hills(rng: &mut Rng, base: f64, amp: f64, trees: bool) -> String {
    let waves: Vec<(f64, f64, f64)> =
        (0..3).map(|k| (rng.range(0.6, 1.0) * amp / (k + 1) as f64, rng.range(1.0, 2.5) * (k + 1) as f64, rng.range(0.0, 2.0 * PI))).collect();
    let y_at = |x: f64| base + waves.iter().map(|(a, f, p)| a * (x / LANDSCAPE_WIDTH * f * PI + p).sin()).sum::<f64>();

    let mut points = Vec::new();
    let mut x = 0.0;
    while x <= LANDSCAPE_WIDTH {
        // A pine every so often: a narrow triangle rising from the hill.
        if trees && rng.chance(0.35) {
            let (h, w) = (rng.range(10.0, 24.0), rng.range(3.0, 5.5));
            let y = y_at(x);
            points.push((x - w, y_at(x - w)));
            points.push((x - w * 0.35, y - h * 0.55));
            points.push((x - w * 0.6, y - h * 0.5));
            points.push((x, y - h));
            points.push((x + w * 0.6, y - h * 0.5));
            points.push((x + w * 0.35, y - h * 0.55));
            points.push((x + w, y_at(x + w)));
            x += w + rng.range(2.0, 14.0);
        } else {
            points.push((x, y_at(x)));
            x += 8.0;
        }
    }
    points.push((LANDSCAPE_WIDTH, y_at(LANDSCAPE_WIDTH)));
    closed_path(&points)
}

/// The outline along `points`, then down to the bottom corners and closed.
fn closed_path(points: &[(f64, f64)]) -> String {
    let mut s = String::new();
    for (i, (x, y)) in points.iter().enumerate() {
        let _ = write!(s, "{} {x:.1} {y:.1} ", if i == 0 { "M" } else { "L" });
    }
    let _ = write!(s, "L {LANDSCAPE_WIDTH} {LANDSCAPE_HEIGHT} L 0 {LANDSCAPE_HEIGHT} Z");
    s
}

fn channels(c: u32) -> [f64; 3] {
    [((c >> 16) & 0xFF) as f64, ((c >> 8) & 0xFF) as f64, (c & 0xFF) as f64]
}

fn pack([r, g, b]: [f64; 3]) -> u32 {
    let ch = |v: f64| v.round().clamp(0.0, 255.0) as u32;
    ch(r) << 16 | ch(g) << 8 | ch(b)
}

/// Blends `a` toward `b` by `t` (0–1).
pub fn mix(a: u32, b: u32, t: f64) -> u32 {
    let (a, b) = (channels(a), channels(b));
    pack([0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t.clamp(0.0, 1.0)))
}

/// The same colour in grey (same perceived brightness).
fn desaturate(c: u32) -> u32 {
    let l = luminance(c) * 255.0;
    pack([l, l, l])
}

/// Perceived brightness, 0–1.
fn luminance(c: u32) -> f64 {
    let [r, g, b] = channels(c);
    (0.299 * r + 0.587 * g + 0.114 * b) / 255.0
}

/// Small deterministic PRNG (xorshift64*).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next_f64(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
    fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_follow_the_sun() {
        let (rise, set) = (Some("2026-11-12T07:40"), Some("2026-11-12T17:10"));
        assert_eq!(Phase::at("2026-11-12T03:00", rise, set, false), Phase::Night);
        assert_eq!(Phase::at("2026-11-12T07:20", rise, set, false), Phase::Dawn);
        assert_eq!(Phase::at("2026-11-12T12:00", rise, set, true), Phase::Day);
        assert_eq!(Phase::at("2026-11-12T17:40", rise, set, false), Phase::Dusk);
        assert_eq!(Phase::at("2026-11-12T21:00", rise, set, false), Phase::Night);
        // No sunrise/sunset known: day or night only.
        assert_eq!(Phase::at("2026-11-12T12:00", None, None, true), Phase::Day);
    }

    #[test]
    fn same_city_same_skyline_and_closed_paths() {
        let a = landscape(Condition::Clear, Phase::Day, 0.5, seed_for("Lyon"));
        let b = landscape(Condition::Clear, Phase::Day, 0.5, seed_for("Lyon"));
        let c = landscape(Condition::Clear, Phase::Day, 0.5, seed_for("Paris"));
        assert_eq!(a, b);
        assert_ne!(a.layers[0].path, c.layers[0].path);
        assert_eq!(a.layers.len(), 4);
        for l in &a.layers {
            assert!(l.path.starts_with("M ") && l.path.ends_with(" Z"));
        }
        assert_eq!(a.ground, a.layers[3].color);
    }

    #[test]
    fn weather_and_time_change_the_scene() {
        let clear = landscape(Condition::Clear, Phase::Day, 0.5, 1);
        let rain = landscape(Condition::Rain, Phase::Day, 0.5, 1);
        let night = landscape(Condition::Clear, Phase::Night, 0.0, 1);
        assert!(clear.orb_visible && !clear.orb_is_moon);
        assert!(!rain.orb_visible, "clouds hide the sun");
        assert!(night.orb_is_moon);
        assert!(luminance(night.sky_top) < luminance(clear.sky_top));
        // Midday sun is higher than at sunrise.
        let morning = landscape(Condition::Clear, Phase::Dawn, 0.0, 1);
        assert!(clear.orb_y < morning.orb_y);
        // Far layers are hazier (closer to the horizon colour) than near ones.
        let diff = |c: u32| {
            let (a, b) = (channels(c), channels(clear.sky_bottom));
            (0..3).map(|i| (a[i] - b[i]).abs()).sum::<f64>()
        };
        assert!(diff(clear.layers[0].color) < diff(clear.layers[3].color));
    }

    #[test]
    fn mixes_colours() {
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
        assert_eq!(mix(0x2F80ED, 0xFFFFFF, 0.0), 0x2F80ED);
    }
}
