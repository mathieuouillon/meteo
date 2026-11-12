use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use meteo_core::{
    Location, MainScreen, Places, cache, fetch_forecast, search_cities, unix_now, updated_ago,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel, Weak};

slint::include_modules!();

mod animation;

#[cfg(target_os = "android")]
mod location_android;
#[cfg(target_os = "macos")]
mod location_macos;

#[cfg(target_os = "android")]
static ANDROID_APP: OnceLock<slint::android::AndroidApp> = OnceLock::new();

thread_local! {
    /// The app state, for results posted back to the UI thread by workers.
    static APP: std::cell::OnceCell<Rc<App>> = const { std::cell::OnceCell::new() };
}

/// Set on Android from the activity's internal data path.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where saved cities and cached forecasts live.
fn data_dir() -> Option<PathBuf> {
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
/// Wait this long after the last keystroke before searching.
const SEARCH_DELAY: Duration = Duration::from_millis(300);

/// App state. Lives on the UI thread; the `Arc` parts are shared with the
/// background threads that fetch forecasts and search for cities.
struct App {
    ui: Weak<MainWindow>,
    dir: Option<PathBuf>,
    places: RefCell<Places>,
    /// When the forecast on screen was downloaded, so a failed refresh can
    /// say how old it is instead of replacing it with an error.
    shown_at: Arc<AtomicI64>,
    /// Bumped by each fetch/search, so results that arrive late (after the
    /// user switched city or kept typing) are dropped.
    fetch_gen: Arc<AtomicU64>,
    search_gen: Arc<AtomicU64>,
    /// The last search results, as places `add_city` can save.
    results: Arc<Mutex<Vec<Location>>>,
}

/// Builds the window, shows the selected city and runs the event loop.
pub fn run() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    let dir = data_dir();
    if let Some(dir) = &dir {
        // Single-city cache from before saved cities existed.
        let _ = std::fs::remove_file(dir.join("forecast.json"));
    }
    let app = Rc::new(App {
        ui: ui.as_weak(),
        places: RefCell::new(dir.as_deref().map(Places::load).unwrap_or_default()),
        dir,
        shown_at: Arc::new(AtomicI64::new(NOTHING_SHOWN)),
        fetch_gen: Arc::default(),
        search_gen: Arc::default(),
        results: Arc::default(),
    });
    APP.with(|a| a.set(app.clone()).ok());
    animation::start(&ui);
    ui.set_can_locate(can_locate());
    app.show_selected();
    // Following the device: update the position quietly at startup if
    // we're already allowed to.
    if app.places.borrow().selected().is_device {
        app.locate(true);
    }

    ui.on_locate({
        let app = app.clone();
        move || app.locate(false)
    });
    ui.on_refresh({
        let app = app.clone();
        move || app.refresh()
    });
    ui.on_select_city({
        let app = app.clone();
        move |i| app.change_places(|p| p.select(i as usize))
    });
    ui.on_add_city({
        let app = app.clone();
        move |i| {
            let city = app.results.lock().unwrap().get(i as usize).cloned();
            if let Some(city) = city {
                app.change_places(|p| p.add(city));
            }
        }
    });
    ui.on_remove_city({
        let app = app.clone();
        move |i| app.change_places(|p| p.remove(i as usize))
    });
    ui.on_search({
        let app = app.clone();
        move |text| app.search(text.to_string())
    });
    ui.run()
}

impl App {
    /// Applies a change to the saved cities, saves them, and switches the
    /// screen if the selected city changed.
    fn change_places(&self, change: impl FnOnce(&mut Places)) {
        let before = self.places.borrow().selected().key();
        change(&mut self.places.borrow_mut());
        if let Some(dir) = &self.dir {
            if let Err(e) = self.places.borrow().save(dir) {
                eprintln!("couldn't save cities: {e}");
            }
        }
        if self.places.borrow().selected().key() != before {
            self.show_selected();
        } else {
            self.update_city_list();
        }
    }

    /// Shows the selected city: its cached forecast at once if there is one,
    /// then a fresh download.
    fn show_selected(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let city = self.places.borrow().selected().clone();
        self.shown_at.store(NOTHING_SHOWN, Ordering::Relaxed);
        match self.dir.as_deref().and_then(|d| cache::load(d, &city, unix_now())) {
            Some(cached) => {
                apply(&ui, &MainScreen::from_forecast(&cached.forecast));
                ui.set_status(updated_ago(unix_now() - cached.fetched_at).into());
                self.shown_at.store(cached.fetched_at, Ordering::Relaxed);
            }
            None => show_loading(&ui, &city),
        }
        self.update_city_list();
        self.refresh();
    }

    fn update_city_list(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let places = self.places.borrow();
        let items: Vec<CityItem> = places
            .cities()
            .iter()
            .enumerate()
            .map(|(i, c)| city_item(c, i == places.selected_index()))
            .collect();
        ui.set_cities(ModelRc::from(Rc::new(VecModel::from(items))));
    }

    /// Downloads the selected city's forecast in the background.
    fn refresh(&self) {
        let city = self.places.borrow().selected().clone();
        let generation = self.fetch_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let (weak, dir, shown_at, fetch_gen) = (
            self.ui.clone(),
            self.dir.clone(),
            self.shown_at.clone(),
            self.fetch_gen.clone(),
        );
        std::thread::spawn(move || {
            let result = block_on(fetch_forecast(&city));
            let fetched_at = unix_now();
            if let (Ok(forecast), Some(dir)) = (&result, &dir) {
                if let Err(e) = cache::save(dir, &city, forecast, fetched_at) {
                    eprintln!("couldn't save the forecast cache: {e}");
                }
                #[cfg(target_os = "android")]
                if let Some(app) = ANDROID_APP.get() {
                    location_android::notify_widgets(app);
                }
            }
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                if fetch_gen.load(Ordering::Relaxed) != generation {
                    return; // the user switched city meanwhile
                }
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
    }

    /// Finds the device's position and selects it as the "📍" city.
    /// `quiet`: no status messages and no permission dialog (startup refresh).
    fn locate(&self, quiet: bool) {
        let Some(ui) = self.ui.upgrade() else { return };
        if !quiet {
            ui.set_search_status("Finding your location…".into());
        }
        let weak = self.ui.clone();
        let finish = move |found: Result<Location, String>| {
            let Some(ui) = weak.upgrade() else { return };
            match found {
                Ok(here) => {
                    APP.with(|a| {
                        if let Some(app) = a.get() {
                            app.change_places(|p| p.set_device(here));
                        }
                    });
                    if !quiet {
                        ui.set_search_status(SharedString::new());
                        ui.set_picker_open(false);
                    }
                }
                Err(message) if !quiet => ui.set_search_status(message.into()),
                Err(_) => {}
            }
        };
        // macOS: CoreLocation calls back on the UI thread.
        #[cfg(target_os = "macos")]
        location_macos::locate(quiet, finish);
        // Android: blocking JNI calls on a worker thread.
        #[cfg(not(target_os = "macos"))]
        std::thread::spawn(move || {
            let found = device_location(quiet);
            let _ = slint::invoke_from_event_loop(move || finish(found));
        });
    }

    /// Searches for cities once the user stops typing for a moment.
    fn search(&self, text: String) {
        let generation = self.search_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let Some(ui) = self.ui.upgrade() else { return };
        if text.trim().chars().count() < 2 {
            ui.set_results(ModelRc::default());
            ui.set_search_status(SharedString::new());
            return;
        }
        ui.set_search_status("Searching…".into());
        let (weak, search_gen, results) = (self.ui.clone(), self.search_gen.clone(), self.results.clone());
        std::thread::spawn(move || {
            std::thread::sleep(SEARCH_DELAY);
            if search_gen.load(Ordering::Relaxed) != generation {
                return; // another key was pressed
            }
            let found = block_on(search_cities(&text));
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                if search_gen.load(Ordering::Relaxed) != generation {
                    return;
                }
                let (status, places) = match found {
                    Ok(p) if p.is_empty() => ("No matching city".to_string(), p),
                    Ok(p) => (String::new(), p),
                    Err(e) => (format!("Search failed: {e}"), Vec::new()),
                };
                let items: Vec<CityItem> = places.iter().map(|c| city_item(c, false)).collect();
                ui.set_results(ModelRc::from(Rc::new(VecModel::from(items))));
                ui.set_search_status(status.into());
                *results.lock().unwrap() = places;
            });
        });
    }
}

/// The device's position and place name (blocking; call from a worker thread).
#[cfg(target_os = "android")]
fn device_location(quiet: bool) -> Result<Location, String> {
    use location_android as loc;
    let app = ANDROID_APP.get().ok_or("Location isn't available")?;
    if !loc::has_permission(app) {
        if quiet {
            return Err(String::new());
        }
        loc::request_permission(app);
        if !loc::wait_for_permission(app, Duration::from_secs(60)) {
            return Err("Location permission wasn't granted".into());
        }
    }
    let (lat, lon) = loc::current_position(app, Duration::from_secs(15))
        .ok_or("Couldn't find your location. Is location turned on?")?;
    let (name, detail) = loc::place_name(app, lat, lon).unwrap_or_else(|| ("My location".into(), String::new()));
    Ok(Location::new(name, detail, lat, lon))
}

#[cfg(not(any(target_os = "android", target_os = "macos")))]
fn device_location(_quiet: bool) -> Result<Location, String> {
    Err("Location isn't available on this platform yet".into())
}

/// Whether to offer "Use my location".
fn can_locate() -> bool {
    #[cfg(target_os = "macos")]
    return location_macos::available();
    #[cfg(not(target_os = "macos"))]
    return cfg!(target_os = "android");
}

/// Runs a future on a small single-threaded runtime (we're on a worker thread).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

fn city_item(c: &Location, selected: bool) -> CityItem {
    CityItem {
        name: c.name.as_str().into(),
        detail: c.detail.as_str().into(),
        selected,
        device: c.is_device,
    }
}

/// A city with no cached forecast yet: its name and a loading state.
fn show_loading(ui: &MainWindow, city: &Location) {
    ui.set_error(SharedString::new());
    ui.set_status(SharedString::new());
    ui.set_city(city.name.as_str().into());
    ui.set_temperature("--°".into());
    ui.set_condition("Loading…".into());
    ui.set_weather_icon(SharedString::new());
    ui.set_high_low(SharedString::new());
    for set in [MainWindow::set_feels_like, MainWindow::set_humidity, MainWindow::set_wind] {
        set(ui, "--".into());
    }
    ui.set_hours(ModelRc::default());
    ui.set_days(ModelRc::default());
    ui.set_line_path(SharedString::new());
    ui.set_area_path(SharedString::new());
}

fn apply(ui: &MainWindow, s: &MainScreen) {
    animation::set_weather(s.scene);
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
    let _ = ANDROID_APP.set(app.clone());
    slint::android::init(app).expect("Slint Android backend");
    run().expect("Slint event loop");
}
