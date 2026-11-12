use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};

use meteo_core::{Location, MainScreen, cache, fetch_forecast, unix_now, updated_ago};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

/// Set on Android from the activity's internal data path.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where the last forecast is cached.
fn cache_dir() -> Option<PathBuf> {
    if let Some(dir) = DATA_DIR.get() {
        return Some(dir.clone());
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Application Support/dev.meteo.app"))
    } else {
        Some(home.join(".cache/meteo"))
    }
}

/// No forecast on screen yet.
const NOTHING_SHOWN: i64 = i64::MIN;

/// Builds the window, shows the cached forecast if any, starts a refresh and
/// runs the event loop.
pub fn run() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let dir = cache_dir();

    // When the forecast on screen was downloaded, so a failed refresh can say
    // how old it is instead of replacing it with an error.
    let shown_at = Arc::new(AtomicI64::new(NOTHING_SHOWN));

    if let Some(cached) = dir.as_deref().and_then(|d| cache::load(d, unix_now())) {
        apply(&ui, &MainScreen::from_forecast(&cached.forecast));
        ui.set_status(updated_ago(unix_now() - cached.fetched_at).into());
        shown_at.store(cached.fetched_at, Ordering::Relaxed);
    }

    let weak = ui.as_weak();
    let load = move || {
        let (weak, dir, shown_at) = (weak.clone(), dir.clone(), shown_at.clone());
        // Network and disk work happen off the UI thread; results are posted back.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let result = rt.block_on(fetch_forecast(&Location::paris()));
            let fetched_at = unix_now();
            if let (Ok(forecast), Some(dir)) = (&result, &dir) {
                if let Err(e) = cache::save(dir, forecast, fetched_at) {
                    eprintln!("couldn't save the forecast cache: {e}");
                }
            }
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                match result {
                    Ok(forecast) => {
                        apply(&ui, &MainScreen::from_forecast(&forecast));
                        ui.set_status(SharedString::new());
                        shown_at.store(fetched_at, Ordering::Relaxed);
                    }
                    Err(e) => match shown_at.load(Ordering::Relaxed) {
                        NOTHING_SHOWN => ui.set_error(format!("Couldn't load the forecast: {e}").into()),
                        t => ui.set_status(format!("Offline · {}", updated_ago(unix_now() - t).to_lowercase()).into()),
                    },
                }
            });
        });
    };

    load();
    ui.on_refresh(load);
    ui.run()
}

fn apply(ui: &MainWindow, s: &MainScreen) {
    ui.set_error(SharedString::new());
    ui.set_city(s.city.as_str().into());
    ui.set_temperature(s.temperature.as_str().into());
    ui.set_condition(s.condition.as_str().into());
    ui.set_weather_icon(s.icon.as_str().into());
    ui.set_high_low(s.high_low.as_str().into());
    ui.set_feels_like(s.feels_like.as_str().into());
    ui.set_humidity(s.humidity.as_str().into());
    ui.set_wind(s.wind.as_str().into());
    ui.set_sky_top(rgb(s.sky_top));
    ui.set_sky_bottom(rgb(s.sky_bottom));
    ui.set_chart_width(s.chart.width as f32);
    ui.set_chart_height(s.chart.height as f32);
    ui.set_line_path(s.chart.line.as_str().into());
    ui.set_area_path(s.chart.area.as_str().into());

    let hours: Vec<HourItem> = s
        .hours
        .iter()
        .map(|h| HourItem {
            label: h.label.as_str().into(),
            temperature: h.temperature.as_str().into(),
            icon: h.icon.as_str().into(),
            precipitation: h.precipitation.as_str().into(),
            x: h.x as f32,
            y: h.y as f32,
        })
        .collect();
    ui.set_hours(ModelRc::from(Rc::new(VecModel::from(hours))));

    let days: Vec<DayItem> = s
        .days
        .iter()
        .map(|d| DayItem {
            label: d.label.as_str().into(),
            icon: d.icon.as_str().into(),
            precipitation: d.precipitation.as_str().into(),
            min: d.min.as_str().into(),
            max: d.max.as_str().into(),
            bar_start: d.bar_start as f32,
            bar_end: d.bar_end as f32,
        })
        .collect();
    ui.set_days(ModelRc::from(Rc::new(VecModel::from(days))));
}

fn rgb(c: u32) -> Color {
    Color::from_argb_encoded(0xff00_0000 | c)
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: slint::android::AndroidApp) {
    if let Some(dir) = app.internal_data_path() {
        let _ = DATA_DIR.set(dir);
    }
    slint::android::init(app).expect("Slint Android backend");
    run().expect("Slint event loop");
}
