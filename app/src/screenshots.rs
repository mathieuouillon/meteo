//! Screenshots of the real UI, drawn with Slint's software renderer from a
//! saved forecast, compared with the reference pictures in
//! `app/tests/screenshots/`. A layout that breaks shows up as a failed test,
//! and CI keeps the pictures it drew.
//!
//! `UPDATE_SCREENSHOTS=1 cargo test -p meteo screenshots` redraws the
//! references after an intended change.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use meteo_core::{ColorMode, Forecast, Lang, MainScreen, Units};
use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, PhysicalSize};

use crate::{CURRENT_LANG, MainWindow, apply, ui_labels};

/// A phone: 412 × 915 logical pixels, drawn at twice that; the main page is
/// drawn taller so it fits whole.
const WIDTH: u32 = 412;
const PHONE_HEIGHT: u32 = 915;
const PAGE_HEIGHT: u32 = 2300;
const SCALE: f32 = 2.0;
/// A pixel counts as changed past this difference on a channel, and the
/// test fails past this share of changed pixels.
const CHANNEL_TOLERANCE: u8 = 24;
const MAX_CHANGED: f64 = 0.002;

/// Slint's clock, moved by hand so animations are always finished.
static CLOCK_MS: AtomicU64 = AtomicU64::new(0);

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
    fn duration_since_start(&self) -> Duration {
        Duration::from_millis(CLOCK_MS.load(Ordering::Relaxed))
    }
}

fn fixture() -> Forecast {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/grenoble.json");
    serde_json::from_slice(&std::fs::read(path).expect("fixture")).expect("fixture parses")
}

#[test]
fn screenshots() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone()))).expect("first platform");
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: SCALE });
    let ui = MainWindow::new().expect("window");
    ui.show().expect("show");
    let forecast = fixture();

    let mut failures = Vec::new();
    for (name, lang, height, setup) in [
        ("main-en", Lang::En, PAGE_HEIGHT, (|_: &MainWindow| {}) as fn(&MainWindow)),
        ("main-fr", Lang::Fr, PAGE_HEIGHT, |_| {}),
        ("settings", Lang::En, PHONE_HEIGHT, |ui| ui.set_settings_open(true)),
        ("radar", Lang::En, PHONE_HEIGHT, |ui| {
            ui.set_radar_time("10:30".into());
            ui.set_radar_status(Lang::En.labels().radar_hint.into());
            ui.set_radar_open(true);
        }),
    ] {
        CURRENT_LANG.store(u8::from(lang == Lang::Fr), Ordering::Relaxed);
        ui.set_settings_open(false);
        ui.set_radar_open(false);
        ui.set_labels(ui_labels(&lang.labels()));
        apply(&ui, &MainScreen::from_forecast(&forecast, lang, Units::Metric), ColorMode::Weather);
        ui.set_updated(lang.updated_ago(5 * 60).into());
        ui.set_settled(true);
        setup(&ui);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (height as f32 * SCALE) as u32));
        let picture = draw(&window);
        if let Some(problem) = check(name, &picture) {
            failures.push(problem);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Moves the clock past every animation and draws a frame.
fn draw(window: &MinimalSoftwareWindow) -> Picture {
    for _ in 0..4 {
        CLOCK_MS.fetch_add(5_000, Ordering::Relaxed);
        slint::platform::update_timers_and_animations();
    }
    window.request_redraw();
    let size = window.size();
    let (w, h) = (size.width as usize, size.height as usize);
    let mut buffer = vec![PremultipliedRgbaColor::default(); w * h];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, w);
    });
    let rgba = buffer
        .iter()
        .flat_map(|p| {
            let un = |c: u8| if p.alpha == 0 { 0 } else { (u16::from(c) * 255 / u16::from(p.alpha)) as u8 };
            [un(p.red), un(p.green), un(p.blue), p.alpha]
        })
        .collect();
    Picture { width: w as u32, height: h as u32, rgba }
}

struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// Saves the picture for CI, then compares it with the reference (or
/// writes the reference when asked to).
fn check(name: &str, picture: &Picture) -> Option<String> {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/screenshots");
    let _ = std::fs::create_dir_all(&out);
    let file = format!("{name}.png");
    save(&out.join(&file), picture);
    let reference = reference_dir().join(&file);
    if std::env::var_os("UPDATE_SCREENSHOTS").is_some() {
        save(&reference, picture);
        return None;
    }
    let Some(expected) = load(&reference) else {
        return Some(format!("{name}: no reference picture; run with UPDATE_SCREENSHOTS=1"));
    };
    if (expected.width, expected.height) != (picture.width, picture.height) {
        return Some(format!(
            "{name}: {}×{} instead of {}×{}",
            picture.width, picture.height, expected.width, expected.height
        ));
    }
    let changed = expected
        .rgba
        .chunks_exact(4)
        .zip(picture.rgba.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > CHANNEL_TOLERANCE))
        .count();
    let share = changed as f64 / f64::from(picture.width * picture.height);
    (share > MAX_CHANGED)
        .then(|| format!("{name}: {:.2}% of the pixels changed (see target/screenshots/{file})", share * 100.0))
}

fn reference_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/screenshots")
}

fn save(path: &Path, picture: &Picture) {
    let _ = std::fs::create_dir_all(path.parent().expect("a folder"));
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("picture file"));
    let mut encoder = png::Encoder::new(file, picture.width, picture.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_compression(png::Compression::High);
    encoder.write_header().expect("PNG header").write_image_data(&picture.rgba).expect("PNG data");
}

fn load(path: &Path) -> Option<Picture> {
    let mut reader = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).ok()?)).read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    (info.color_type == png::ColorType::Rgba).then_some(Picture { width: info.width, height: info.height, rgba: buf })
}

/// Saves today's forecast for Grenoble as the fixture (network).
#[test]
#[ignore = "network; refreshes the fixture"]
fn update_fixture() {
    let place = meteo_core::Location {
        name: "Grenoble".into(),
        detail: "Auvergne-Rhône-Alpes, France".into(),
        latitude: 45.19,
        longitude: 5.72,
        is_device: false,
        country_code: "FR".into(),
        areas: vec!["Isère".into(), "Auvergne-Rhône-Alpes".into()],
    };
    // A folder for the normals and records, which come with the forecast.
    let dir = std::env::temp_dir().join("meteo-fixture");
    std::fs::create_dir_all(&dir).unwrap();
    let f = crate::block_on(meteo_core::extras::download(Some(&dir), &place, meteo_core::Source::BestMatch))
        .expect("download");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/grenoble.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec_pretty(&f).unwrap()).unwrap();
}
