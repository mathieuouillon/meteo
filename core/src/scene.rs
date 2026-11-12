//! Animated weather behind the forecast: rain, snow, clouds, fog, stars and
//! lightning, as a small particle simulation.
//!
//! The UI calls [`Scene::step`] every frame and draws the [`SceneFrame`]:
//! particle layers come out as SVG-style path strings (one path per layer,
//! so the UI draws hundreds of drops with a single element), clouds and fog
//! as a few positioned shapes.
//!
//! Units are logical pixels and seconds; y grows downwards.

use std::f64::consts::TAU;
use std::fmt::Write;

use serde::Serialize;

use crate::condition::Condition;

/// What to animate, derived from the current weather.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SceneSpec {
    /// 0 = none, 1 = downpour.
    pub rain: f64,
    /// 0 = none, 1 = heavy snowfall.
    pub snow: f64,
    pub clouds: u8,
    /// Grey storm clouds instead of white ones.
    pub dark_clouds: bool,
    pub fog: bool,
    pub stars: bool,
    pub lightning: bool,
    /// Horizontal wind on screen in px/s (positive = to the right).
    pub wind: f64,
}

impl SceneSpec {
    /// `wind_kmh` and `wind_from_deg` are the forecast's wind speed and the
    /// direction it blows *from* (meteorological convention).
    pub fn for_weather(condition: Condition, is_day: bool, wind_kmh: f64, wind_from_deg: f64) -> Self {
        // Wind from the west (270°) blows to the east, i.e. to the right.
        let toward_right = -wind_from_deg.to_radians().sin();
        let wind = (wind_kmh * 4.0 * toward_right).clamp(-160.0, 160.0);
        let none = Self { rain: 0.0, snow: 0.0, clouds: 0, dark_clouds: false, fog: false, stars: false, lightning: false, wind };
        use Condition::*;
        match condition {
            Clear => Self { clouds: u8::from(is_day), stars: !is_day, ..none },
            PartlyCloudy => Self { clouds: 3, stars: !is_day, ..none },
            Cloudy => Self { clouds: 6, ..none },
            Fog => Self { clouds: 2, fog: true, ..none },
            Drizzle => Self { rain: 0.3, clouds: 4, ..none },
            Rain => Self { rain: 0.75, clouds: 5, dark_clouds: true, ..none },
            Snow => Self { snow: 0.7, clouds: 4, ..none },
            Thunderstorm => Self { rain: 1.0, clouds: 6, dark_clouds: true, lightning: true, ..none },
        }
    }
}

/// One frame to draw.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SceneFrame {
    /// Rain streaks, far (thin, faint) and near (thicker, brighter) layers.
    pub rain_far: String,
    pub rain_near: String,
    /// Droplets bouncing off the bottom edge (round dots).
    pub splashes: String,
    /// Snowflakes as round dots, small and large.
    pub snow_small: String,
    pub snow_large: String,
    /// Stars grouped by current brightness (dim, medium, bright).
    pub stars: [String; 3],
    /// The lightning bolt while it's visible, else empty.
    pub bolt: String,
    /// 0–1, how much the sky lights up from lightning.
    pub flash: f64,
    pub clouds: Vec<CloudView>,
    pub fog: Vec<FogView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CloudView {
    pub x: f64,
    pub y: f64,
    /// Multiplies the cloud's base size.
    pub scale: f64,
    pub opacity: f64,
    /// Grey storm cloud rather than a white one.
    pub dark: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FogView {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub opacity: f64,
}

/// Base cloud size before `scale`; the UI's cloud shape is drawn in this box.
pub const CLOUD_WIDTH: f64 = 180.0;
pub const CLOUD_HEIGHT: f64 = 80.0;

const GRAVITY: f64 = 2400.0;
/// Large time steps (app resumed, slow frame) are clamped so nothing jumps.
const MAX_DT: f64 = 0.05;

#[derive(Debug, Clone)]
struct Drop {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    /// Depth 0.5 (far) – 1 (near): scales speed, length and thickness.
    z: f64,
}

#[derive(Debug, Clone)]
struct Splash {
    x: f64,
    y: f64,
    vx: f64,
    vy: f64,
    life: f64,
}

#[derive(Debug, Clone)]
struct Flake {
    x: f64,
    y: f64,
    z: f64,
    phase: f64,
    sway_hz: f64,
}

#[derive(Debug, Clone)]
struct Cloud {
    x: f64,
    y: f64,
    scale: f64,
}

#[derive(Debug, Clone)]
struct Star {
    x: f64,
    y: f64,
    phase: f64,
    hz: f64,
}

#[derive(Debug, Clone)]
struct Band {
    x: f64,
    y: f64,
    width: f64,
    speed: f64,
}

pub struct Scene {
    spec: SceneSpec,
    width: f64,
    height: f64,
    rng: Rng,
    time: f64,
    drops: Vec<Drop>,
    splashes: Vec<Splash>,
    flakes: Vec<Flake>,
    clouds: Vec<Cloud>,
    stars: Vec<Star>,
    fog: Vec<Band>,
    flash: f64,
    next_flash: f64,
    /// A second, quicker flicker shortly after the main flash.
    flicker_at: Option<f64>,
    bolt: Vec<(f64, f64)>,
}

impl Scene {
    pub fn new(spec: SceneSpec, width: f64, height: f64, seed: u64) -> Self {
        let mut scene = Self {
            spec,
            width: width.max(1.0),
            height: height.max(1.0),
            rng: Rng::new(seed),
            time: 0.0,
            drops: Vec::new(),
            splashes: Vec::new(),
            flakes: Vec::new(),
            clouds: Vec::new(),
            stars: Vec::new(),
            fog: Vec::new(),
            flash: 0.0,
            next_flash: 0.0,
            flicker_at: None,
            bolt: Vec::new(),
        };
        scene.populate();
        scene
    }

    pub fn spec(&self) -> SceneSpec {
        self.spec
    }

    /// Switches to new weather, keeping the clouds that are still wanted so
    /// the change isn't abrupt.
    pub fn set_spec(&mut self, spec: SceneSpec) {
        if spec != self.spec {
            let clouds = std::mem::take(&mut self.clouds);
            self.spec = spec;
            self.populate();
            for (new, old) in self.clouds.iter_mut().zip(clouds) {
                *new = old;
            }
        }
    }

    /// Adapts to a new window size (particles are re-spread if it changed a lot).
    pub fn resize(&mut self, width: f64, height: f64) {
        let (width, height) = (width.max(1.0), height.max(1.0));
        if (width - self.width).abs() > 1.0 || (height - self.height).abs() > 1.0 {
            self.width = width;
            self.height = height;
            self.populate();
        }
    }

    fn populate(&mut self) {
        let (w, h) = (self.width, self.height);
        let area = w * h;
        let s = self.spec;

        let drop_count = (s.rain * area / 2500.0).min(320.0) as usize;
        self.drops = (0..drop_count)
            .map(|_| {
                let mut d = self.new_drop();
                d.y = self.rng.range(-h * 0.2, h); // already falling at start
                d.vy = d.z * 1100.0;
                d
            })
            .collect();
        self.splashes.clear();

        let flake_count = (s.snow * area / 3500.0).min(220.0) as usize;
        self.flakes = (0..flake_count)
            .map(|_| {
                let mut f = self.new_flake();
                f.y = self.rng.range(0.0, h);
                f
            })
            .collect();

        self.clouds = (0..s.clouds)
            .map(|i| {
                let scale = self.rng.range(0.7, 1.4);
                Cloud {
                    // Spread across the width, then jittered.
                    x: (i as f64 + self.rng.range(0.0, 0.8)) / s.clouds as f64 * (w + CLOUD_WIDTH) - CLOUD_WIDTH,
                    y: self.rng.range(-CLOUD_HEIGHT * 0.3, h * 0.28),
                    scale,
                }
            })
            .collect();

        let star_count = if s.stars { (area / 7000.0).min(140.0) as usize } else { 0 };
        self.stars = (0..star_count)
            .map(|_| Star {
                x: self.rng.range(0.0, w),
                y: self.rng.range(0.0, h * 0.65),
                phase: self.rng.range(0.0, TAU),
                hz: self.rng.range(0.15, 0.6),
            })
            .collect();

        self.fog = if s.fog {
            (0..5)
                .map(|i| Band {
                    width: self.rng.range(w * 0.8, w * 1.4),
                    x: self.rng.range(-w * 0.5, w * 0.3),
                    y: h * (0.15 + 0.17 * i as f64),
                    speed: self.rng.range(6.0, 16.0) * if i % 2 == 0 { 1.0 } else { -1.0 },
                })
                .collect()
        } else {
            Vec::new()
        };

        self.flash = 0.0;
        self.flicker_at = None;
        self.bolt.clear();
        self.next_flash = self.rng.range(1.5, 5.0);
    }

    fn new_drop(&mut self) -> Drop {
        let z = self.rng.range(0.5, 1.0);
        // Start wide enough that wind blowing sideways still fills the screen.
        let drift = self.spec.wind * self.height / 1000.0;
        let x = self.rng.range(-drift.max(0.0) - 20.0, self.width - drift.min(0.0) + 20.0);
        Drop { x, y: self.rng.range(-80.0, -10.0), vx: self.spec.wind * z, vy: z * 600.0, z }
    }

    fn new_flake(&mut self) -> Flake {
        Flake {
            x: self.rng.range(-20.0, self.width + 20.0),
            y: self.rng.range(-30.0, -5.0),
            z: self.rng.range(0.4, 1.0),
            phase: self.rng.range(0.0, TAU),
            sway_hz: self.rng.range(0.2, 0.6),
        }
    }

    /// Advances the simulation by `dt` seconds.
    pub fn step(&mut self, dt: f64) {
        let dt = dt.clamp(0.0, MAX_DT);
        self.time += dt;
        let (w, h, wind) = (self.width, self.height, self.spec.wind);

        // Rain: gravity with air drag up to a terminal speed that depends on
        // the drop's size (depth), pushed sideways toward the wind speed.
        for i in 0..self.drops.len() {
            let d = &mut self.drops[i];
            let terminal = 900.0 + 500.0 * d.z;
            d.vy += GRAVITY * (1.0 - (d.vy / terminal).powi(2)) * dt;
            d.vx += (wind * d.z - d.vx) * 3.0 * dt;
            d.x += d.vx * dt;
            d.y += d.vy * dt;
            if d.y > h {
                let (x, vx, z) = (d.x, d.vx, d.z);
                if z > 0.7 && self.rng.chance(0.6) {
                    for _ in 0..2 {
                        self.splashes.push(Splash {
                            x,
                            y: h - 2.0,
                            vx: vx * 0.2 + self.rng.range(-60.0, 60.0),
                            vy: -self.rng.range(120.0, 260.0),
                            life: self.rng.range(0.2, 0.4),
                        });
                    }
                }
                self.drops[i] = self.new_drop();
            }
        }
        for s in &mut self.splashes {
            s.vy += GRAVITY * 0.5 * dt;
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            s.life -= dt;
        }
        self.splashes.retain(|s| s.life > 0.0 && s.y <= h);

        // Snow: slow terminal fall, swaying on a per-flake rhythm, carried by the wind.
        let t = self.time;
        for i in 0..self.flakes.len() {
            let f = &mut self.flakes[i];
            let sway = 28.0 * (TAU * f.sway_hz * t + f.phase).sin();
            f.x += (wind * 0.6 * f.z + sway) * dt;
            f.y += (35.0 + 55.0 * f.z) * dt;
            if f.x < -30.0 {
                f.x += w + 60.0;
            } else if f.x > w + 30.0 {
                f.x -= w + 60.0;
            }
            if f.y > h + 10.0 {
                self.flakes[i] = self.new_flake();
            }
        }

        // Clouds drift with the wind (a little even when calm); bigger ones
        // look closer and move faster (parallax). They wrap around.
        let drift = if wind.abs() < 8.0 { 8.0 } else { wind * 0.35 };
        for c in &mut self.clouds {
            c.x += drift * c.scale * dt;
            let cw = CLOUD_WIDTH * c.scale;
            if c.x > w + 10.0 {
                c.x = -cw - 10.0;
            } else if c.x < -cw - 10.0 {
                c.x = w + 10.0;
            }
        }

        for b in &mut self.fog {
            b.x += b.speed * dt;
            if b.x > w * 0.6 || b.x + b.width < w * 0.4 {
                b.speed = -b.speed;
            }
        }

        // Lightning: a flash, a quick second flicker, then a random pause.
        self.flash *= (-6.0 * dt).exp();
        if self.spec.lightning {
            if self.time >= self.next_flash {
                self.flash = 1.0;
                self.flicker_at = Some(self.time + self.rng.range(0.1, 0.18));
                self.bolt = self.make_bolt();
                self.next_flash = self.time + self.rng.range(3.0, 9.0);
            }
            if self.flicker_at.is_some_and(|at| self.time >= at) {
                self.flash = self.flash.max(0.7);
                self.flicker_at = None;
            }
        }
        if self.flash < 0.35 {
            self.bolt.clear();
        }
    }

    /// A jagged line from the top down to somewhere in the upper half.
    fn make_bolt(&mut self) -> Vec<(f64, f64)> {
        let mut x = self.rng.range(self.width * 0.2, self.width * 0.8);
        let bottom = self.height * self.rng.range(0.35, 0.6);
        let mut y = 0.0;
        let mut points = vec![(x, y)];
        while y < bottom {
            y += self.rng.range(18.0, 45.0);
            x += self.rng.range(-28.0, 28.0);
            points.push((x, y.min(bottom)));
        }
        points
    }

    /// The current frame, ready to draw.
    pub fn frame(&self) -> SceneFrame {
        let mut f = SceneFrame { flash: self.flash, ..SceneFrame::default() };

        for d in &self.drops {
            // A streak along the velocity: motion blur of ~1/40 s.
            let (dx, dy) = (d.vx / 40.0, d.vy / 40.0);
            let path = if d.z > 0.75 { &mut f.rain_near } else { &mut f.rain_far };
            line(path, d.x - dx, d.y - dy, d.x, d.y);
        }
        for s in &self.splashes {
            dot(&mut f.splashes, s.x, s.y);
        }
        for fl in &self.flakes {
            dot(if fl.z > 0.7 { &mut f.snow_large } else { &mut f.snow_small }, fl.x, fl.y);
        }
        for s in &self.stars {
            let brightness = 0.5 + 0.5 * (TAU * s.hz * self.time + s.phase).sin();
            let group = ((brightness * 3.0) as usize).min(2);
            dot(&mut f.stars[group], s.x, s.y);
        }
        for (i, &(x, y)) in self.bolt.iter().enumerate() {
            let _ = write!(f.bolt, "{}{x:.1} {y:.1} ", if i == 0 { "M " } else { "L " });
        }

        let dark = self.spec.dark_clouds;
        f.clouds = self
            .clouds
            .iter()
            // Bigger (closer) clouds are a little more opaque.
            .map(|c| CloudView { x: c.x, y: c.y, scale: c.scale, opacity: if dark { 0.55 } else { 0.28 } * (0.7 + 0.3 * c.scale), dark })
            .collect();
        f.fog = self
            .fog
            .iter()
            .map(|b| FogView { x: b.x, y: b.y, width: b.width, height: self.height * 0.16, opacity: 0.22 })
            .collect();
        f
    }
}

fn line(path: &mut String, x1: f64, y1: f64, x2: f64, y2: f64) {
    let _ = write!(path, "M {x1:.1} {y1:.1} L {x2:.1} {y2:.1} ");
}

/// A tiny segment: with round line caps it draws as a dot.
fn dot(path: &mut String, x: f64, y: f64) {
    let _ = write!(path, "M {x:.1} {y:.1} l 0.1 0 ");
}

/// Small deterministic PRNG (xorshift64*), so the scene needs no extra crate
/// and tests are reproducible.
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

    fn run(scene: &mut Scene, seconds: f64) {
        for _ in 0..(seconds * 60.0) as usize {
            scene.step(1.0 / 60.0);
        }
    }

    #[test]
    fn weather_maps_to_layers() {
        let rain = SceneSpec::for_weather(Condition::Rain, true, 10.0, 270.0);
        assert!(rain.rain > 0.0 && rain.snow == 0.0 && !rain.lightning);
        let storm = SceneSpec::for_weather(Condition::Thunderstorm, true, 10.0, 270.0);
        assert!(storm.lightning);
        let night = SceneSpec::for_weather(Condition::Clear, false, 0.0, 0.0);
        assert!(night.stars && night.clouds == 0);
        // West wind blows to the right, east wind to the left.
        assert!(rain.wind > 0.0);
        assert!(SceneSpec::for_weather(Condition::Rain, true, 10.0, 90.0).wind < 0.0);
    }

    #[test]
    fn rain_falls_recycles_and_follows_the_wind() {
        let spec = SceneSpec::for_weather(Condition::Rain, true, 30.0, 270.0);
        let mut scene = Scene::new(spec, 400.0, 800.0, 7);
        let count = scene.drops.len();
        assert!(count > 50);
        run(&mut scene, 5.0);
        assert_eq!(scene.drops.len(), count, "drops are recycled, not lost");
        assert!(scene.drops.iter().all(|d| d.y <= 800.0 + 1.0));
        // Near terminal speed, and drifting right like the wind.
        let mean_vy = scene.drops.iter().map(|d| d.vy).sum::<f64>() / count as f64;
        let mean_vx = scene.drops.iter().map(|d| d.vx).sum::<f64>() / count as f64;
        assert!(mean_vy > 900.0, "mean vy {mean_vy}");
        assert!(mean_vx > 0.0);
        assert!(!scene.frame().rain_near.is_empty());
    }

    #[test]
    fn snow_stays_on_screen_horizontally() {
        let spec = SceneSpec::for_weather(Condition::Snow, true, 40.0, 90.0);
        let mut scene = Scene::new(spec, 400.0, 800.0, 3);
        run(&mut scene, 20.0);
        assert!(scene.flakes.iter().all(|f| (-31.0..=431.0).contains(&f.x)));
        let frame = scene.frame();
        assert!(frame.rain_far.is_empty() && frame.rain_near.is_empty());
        assert!(!frame.snow_small.is_empty() || !frame.snow_large.is_empty());
    }

    #[test]
    fn clouds_wrap_around() {
        let spec = SceneSpec::for_weather(Condition::Cloudy, true, 20.0, 270.0);
        let mut scene = Scene::new(spec, 400.0, 800.0, 11);
        run(&mut scene, 120.0);
        for c in scene.frame().clouds {
            assert!(c.x >= -CLOUD_WIDTH * c.scale - 11.0 && c.x <= 411.0, "x {}", c.x);
        }
    }

    #[test]
    fn lightning_only_in_storms() {
        let storm = SceneSpec::for_weather(Condition::Thunderstorm, true, 10.0, 270.0);
        let mut scene = Scene::new(storm, 400.0, 800.0, 5);
        let mut max_flash: f64 = 0.0;
        for _ in 0..600 {
            scene.step(1.0 / 60.0);
            max_flash = max_flash.max(scene.frame().flash);
        }
        assert!(max_flash > 0.9, "a storm flashes within 10 s");
        // The bolt is drawn while the flash is bright.
        let mut scene = Scene::new(storm, 400.0, 800.0, 5);
        let mut saw_bolt = false;
        for _ in 0..600 {
            scene.step(1.0 / 60.0);
            let f = scene.frame();
            saw_bolt |= f.flash > 0.5 && f.bolt.starts_with("M ");
            assert!(f.bolt.is_empty() || f.flash >= 0.35);
        }
        assert!(saw_bolt);

        let rain = SceneSpec::for_weather(Condition::Rain, true, 10.0, 270.0);
        let mut scene = Scene::new(rain, 400.0, 800.0, 5);
        run(&mut scene, 10.0);
        assert_eq!(scene.frame().flash, 0.0);
    }

    #[test]
    fn long_pauses_dont_teleport_particles() {
        let spec = SceneSpec::for_weather(Condition::Rain, true, 0.0, 0.0);
        let mut scene = Scene::new(spec, 400.0, 800.0, 9);
        let before: Vec<f64> = scene.drops.iter().map(|d| d.y).collect();
        scene.step(30.0); // e.g. the app was in the background
        for (d, y0) in scene.drops.iter().zip(before) {
            assert!(d.y - y0 < 100.0 || d.y < 0.0);
        }
    }
}
