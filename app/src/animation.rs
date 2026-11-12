//! Drives the animated weather background: steps `meteo_core::Scene` about
//! 60 times a second and hands each frame to the Slint UI.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use meteo_core::{Scene, SceneFrame, SceneSpec};
use slint::{ComponentHandle, Model, ModelRc, VecModel, Weak};

use crate::{CloudItem, FogItem, MainWindow};

struct State {
    scene: Option<Scene>,
    /// Weather to show once the window size is known.
    spec: Option<SceneSpec>,
    last: Instant,
    clouds: Rc<VecModel<CloudItem>>,
    fog: Rc<VecModel<FogItem>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        scene: None,
        spec: None,
        last: Instant::now(),
        clouds: Rc::new(VecModel::default()),
        fog: Rc::new(VecModel::default()),
    });
    static TIMER: slint::Timer = slint::Timer::default();
}

/// Starts the animation loop for `ui`. Call once, on the UI thread.
pub fn start(ui: &MainWindow) {
    STATE.with(|s| {
        let s = s.borrow();
        ui.set_clouds(ModelRc::from(s.clouds.clone()));
        ui.set_fog(ModelRc::from(s.fog.clone()));
    });
    let weak = ui.as_weak();
    TIMER.with(|t| t.start(slint::TimerMode::Repeated, frame_interval(None), move || tick(&weak)));
}

/// 60 fps for falling particles and lightning; slow drifting clouds, fog
/// and stars look the same at 30 fps for half the work.
fn frame_interval(spec: Option<SceneSpec>) -> Duration {
    let fast = spec.is_some_and(|s| s.rain > 0.0 || s.snow > 0.0 || s.lightning);
    Duration::from_millis(if fast { 16 } else { 33 })
}

/// Changes the weather being animated (after a forecast is shown).
pub fn set_weather(spec: SceneSpec) {
    let spec = forced_weather().unwrap_or(spec);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.spec = Some(spec);
        if let Some(scene) = &mut s.scene {
            scene.set_spec(spec);
        }
    });
    TIMER.with(|t| {
        let interval = frame_interval(Some(spec));
        if t.running() && t.interval() != interval {
            t.set_interval(interval);
        }
    });
}

/// For trying the animations: `METEO_WEATHER=storm cargo run -p meteo`
/// (clear, night, partly, cloudy, fog, drizzle, rain, snow, storm).
fn forced_weather() -> Option<SceneSpec> {
    use meteo_core::Condition::*;
    let name = std::env::var("METEO_WEATHER").ok()?;
    let (condition, day) = match name.as_str() {
        "clear" => (Clear, true),
        "night" => (Clear, false),
        "partly" => (PartlyCloudy, true),
        "cloudy" => (Cloudy, true),
        "fog" => (Fog, true),
        "drizzle" => (Drizzle, true),
        "rain" => (Rain, true),
        "snow" => (Snow, true),
        "storm" => (Thunderstorm, true),
        _ => return None,
    };
    Some(SceneSpec::for_weather(condition, day, 20.0, 250.0))
}

fn tick(weak: &Weak<MainWindow>) {
    let Some(ui) = weak.upgrade() else { return };
    let window = ui.window();
    let size = window.size().to_logical(window.scale_factor());
    let (w, h) = (f64::from(size.width), f64::from(size.height));

    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let now = Instant::now();
        let dt = now.duration_since(s.last).as_secs_f64();
        s.last = now;

        let Some(spec) = s.spec else { return };
        // Nothing moves behind the city picker.
        if ui.get_picker_open() || w < 1.0 {
            return;
        }
        let scene = s.scene.get_or_insert_with(|| Scene::new(spec, w, h, seed()));
        scene.resize(w, h);
        scene.step(dt);
        let frame = scene.frame();
        show(&ui, &s, frame);
    });
}

fn show(ui: &MainWindow, s: &State, f: SceneFrame) {
    ui.set_rain_far(f.rain_far.into());
    ui.set_rain_near(f.rain_near.into());
    ui.set_splashes(f.splashes.into());
    ui.set_snow_small(f.snow_small.into());
    ui.set_snow_large(f.snow_large.into());
    let [dim, mid, bright] = f.stars;
    ui.set_stars_dim(dim.into());
    ui.set_stars_mid(mid.into());
    ui.set_stars_bright(bright.into());
    ui.set_bolt(f.bolt.into());
    ui.set_flash(f.flash as f32);

    let clouds = f.clouds.iter().map(|c| CloudItem {
        x: c.x as f32,
        y: c.y as f32,
        scale: c.scale as f32,
        opacity: c.opacity as f32,
        dark: c.dark,
    });
    update_model(&s.clouds, clouds);
    let fog = f.fog.iter().map(|b| FogItem {
        x: b.x as f32,
        y: b.y as f32,
        width: b.width as f32,
        height: b.height as f32,
        opacity: b.opacity as f32,
    });
    update_model(&s.fog, fog);
}

/// Updates rows in place (cheaper than a new model every frame).
fn update_model<T: Clone + 'static>(model: &VecModel<T>, items: impl ExactSizeIterator<Item = T>) {
    if model.row_count() != items.len() {
        model.set_vec(items.collect::<Vec<_>>());
    } else {
        for (i, item) in items.enumerate() {
            model.set_row_data(i, item);
        }
    }
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}
