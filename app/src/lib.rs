use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicI64, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use meteo_core::{
    ColorMode, Forecast, Lang, LanguageChoice, Location, MainScreen, Palette, Places, Settings, Source, Units, cache,
    extras, search_cities, unix_now,
};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel, Weak};

slint::include_modules!();

mod animation;
mod theme;

#[cfg(target_os = "android")]
mod android;

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
    // Desktop development preview (`cargo run -p meteo`).
    Some(home.join(".cache/meteo"))
}

/// The phone's language, detected once.
static SYSTEM_LANG: OnceLock<Lang> = OnceLock::new();
/// The UI's language: the phone's or the one chosen in the settings.
static CURRENT_LANG: AtomicU8 = AtomicU8::new(0);

fn system_lang() -> Lang {
    *SYSTEM_LANG.get_or_init(|| {
        #[cfg(target_os = "android")]
        if let Some(code) = ANDROID_APP.get().and_then(android::language) {
            return Lang::from_code(&code);
        }
        // Desktop preview: METEO_LANG=fr, or the usual LANG.
        let code = std::env::var("METEO_LANG").or_else(|_| std::env::var("LANG")).unwrap_or_default();
        Lang::from_code(&code)
    })
}

fn lang() -> Lang {
    if CURRENT_LANG.load(Ordering::Relaxed) == 1 { Lang::Fr } else { Lang::En }
}

fn set_lang(settings: &Settings) {
    let l = settings.lang(system_lang());
    CURRENT_LANG.store(u8::from(l == Lang::Fr), Ordering::Relaxed);
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
    settings: RefCell<Settings>,
    /// When the forecast on screen was downloaded, so a failed refresh can
    /// say how old it is instead of replacing it with an error.
    shown_at: Arc<AtomicI64>,
    /// Bumped by each fetch/search, so results that arrive late (after the
    /// user switched city or kept typing) are dropped.
    fetch_gen: Arc<AtomicU64>,
    search_gen: Arc<AtomicU64>,
    /// The last search results, as places `add_city` can save.
    results: Arc<Mutex<Vec<Location>>>,
    /// The forecast on screen, to redraw it when a setting changes.
    shown: RefCell<Option<Forecast>>,
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
        settings: RefCell::new(dir.as_deref().map(Settings::load).unwrap_or_default()),
        dir,
        shown_at: Arc::new(AtomicI64::new(NOTHING_SHOWN)),
        fetch_gen: Arc::default(),
        search_gen: Arc::default(),
        results: Arc::default(),
        shown: RefCell::new(None),
    });
    APP.with(|a| a.set(app.clone()).ok());
    set_lang(&app.settings.borrow());
    animation::start(&ui);
    ui.set_labels(ui_labels(&lang().labels()));
    app.update_settings_ui();
    // The wind compass follows the phone's heading (Android's rotation sensor).
    #[cfg(target_os = "android")]
    {
        let weak = ui.as_weak();
        let last = Arc::new(std::sync::Mutex::new(f32::NAN));
        android::start_compass(move |heading| {
            // Only redraw for a change of a degree or more.
            let mut l = last.lock().unwrap();
            if (heading - *l).abs() < 1.0 {
                return;
            }
            *l = heading;
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    ui.set_compass_heading(heading);
                }
            });
        });
    }
    // "Updated 3 min ago" keeps counting while the app is open.
    UPDATED_TIMER.with(|t| {
        let app = app.clone();
        t.start(slint::TimerMode::Repeated, Duration::from_secs(30), move || app.update_updated());
    });
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
    ui.on_open_url(|url| open_url(&url));
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
    ui.on_select_source({
        let app = app.clone();
        move |i| {
            if let Some(&source) = Source::ALL.get(i as usize) {
                app.set_source(source);
            }
        }
    });
    ui.on_search({
        let app = app.clone();
        move |text| app.search(text.to_string())
    });
    ui.on_set_units({
        let app = app.clone();
        move |i| app.change_settings(|s| s.units = if i == 1 { Units::Imperial } else { Units::Metric })
    });
    ui.on_set_colors({
        let app = app.clone();
        move |i| app.change_settings(|s| s.colors = if i == 1 { ColorMode::Wallpaper } else { ColorMode::Weather })
    });
    ui.on_set_language({
        let app = app.clone();
        move |i| {
            app.change_settings(|s| {
                s.language = match i {
                    1 => LanguageChoice::English,
                    2 => LanguageChoice::French,
                    _ => LanguageChoice::System,
                }
            })
        }
    });
    ui.on_set_morning_summary({
        let app = app.clone();
        move |on| {
            app.change_settings(|s| s.morning_summary = on);
            if on {
                request_notifications();
            }
        }
    });
    ui.on_set_rain_alerts({
        let app = app.clone();
        move |on| {
            app.change_settings(|s| s.rain_alerts = on);
            if on {
                request_notifications();
            }
        }
    });
    ui.on_set_warning_alerts({
        let app = app.clone();
        move |on| {
            app.change_settings(|s| s.warning_alerts = on);
            if on {
                request_notifications();
            }
        }
    });
    ui.run()
}

/// Opens a web page in the phone's browser (the desktop's on the desktop).
fn open_url(url: &str) {
    #[cfg(target_os = "android")]
    if let Some(app) = ANDROID_APP.get() {
        android::open_url(app, url);
    }
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

thread_local! {
    static UPDATED_TIMER: slint::Timer = slint::Timer::default();
}

/// Asks for permission to post notifications (Android 13+).
fn request_notifications() {
    #[cfg(target_os = "android")]
    if let Some(app) = ANDROID_APP.get() {
        let app = app.clone();
        std::thread::spawn(move || {
            android::request_notification_permission(&app);
            // Check right away rather than at the next 15-minute run.
            android::check_notifications_now(&app);
        });
    }
}

impl App {
    /// Applies a change to the settings, saves them, and redraws.
    fn change_settings(&self, change: impl FnOnce(&mut Settings)) {
        change(&mut self.settings.borrow_mut());
        if let Some(dir) = &self.dir
            && let Err(e) = self.settings.borrow().save(dir)
        {
            eprintln!("couldn't save settings: {e}");
        }
        set_lang(&self.settings.borrow());
        if let Some(ui) = self.ui.upgrade() {
            ui.set_labels(ui_labels(&lang().labels()));
        }
        self.update_settings_ui();
        self.update_source_list();
        self.render();
    }

    fn update_settings_ui(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let s = self.settings.borrow();
        ui.set_units_index(i32::from(s.units == Units::Imperial));
        ui.set_colors_index(i32::from(s.colors == ColorMode::Wallpaper));
        ui.set_language_index(match s.language {
            LanguageChoice::System => 0,
            LanguageChoice::English => 1,
            LanguageChoice::French => 2,
        });
        ui.set_morning_summary(s.morning_summary);
        ui.set_rain_alerts(s.rain_alerts);
        ui.set_warning_alerts(s.warning_alerts);
        ui.set_can_use_wallpaper(cfg!(target_os = "android"));
    }

    /// Draws the forecast on screen again (after a setting changed).
    fn render(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let settings = self.settings.borrow();
        if let Some(f) = self.shown.borrow().as_ref() {
            apply(&ui, &MainScreen::from_forecast(f, lang(), settings.units), settings.colors);
        }
        drop(settings);
        self.update_updated();
    }

    /// "Updated 3 min ago", from when the forecast on screen was downloaded.
    fn update_updated(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        match self.shown_at.load(Ordering::Relaxed) {
            NOTHING_SHOWN => {}
            t => ui.set_updated(lang().updated_ago(unix_now() - t).into()),
        }
    }

    /// Shows a forecast (from the cache or just downloaded).
    fn show(&self, forecast: Forecast, fetched_at: i64) {
        *self.shown.borrow_mut() = Some(forecast);
        self.shown_at.store(fetched_at, Ordering::Relaxed);
        self.render();
    }

    /// Applies a change to the saved cities, saves them, and switches the
    /// screen if the selected city changed.
    fn change_places(&self, change: impl FnOnce(&mut Places)) {
        let before = self.places.borrow().selected().key();
        change(&mut self.places.borrow_mut());
        if let Some(dir) = &self.dir
            && let Err(e) = self.places.borrow().save(dir)
        {
            eprintln!("couldn't save cities: {e}");
        }
        if self.places.borrow().selected().key() != before {
            self.show_selected();
        } else {
            self.update_city_list();
        }
    }

    /// Switches the weather model, saves the choice and reloads the forecast.
    fn set_source(&self, source: Source) {
        if self.settings.borrow().source == source {
            return;
        }
        self.settings.borrow_mut().source = source;
        if let Some(dir) = &self.dir
            && let Err(e) = self.settings.borrow().save(dir)
        {
            eprintln!("couldn't save settings: {e}");
        }
        self.show_selected();
    }

    fn update_source_list(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let current = self.settings.borrow().source;
        let items: Vec<CityItem> = Source::ALL
            .iter()
            .map(|&s| CityItem {
                name: s.name(lang()).into(),
                detail: s.description(lang()).into(),
                selected: s == current,
                device: false,
            })
            .collect();
        ui.set_sources(ModelRc::from(Rc::new(VecModel::from(items))));
        ui.set_source_name(current.name(lang()).into());
    }

    /// Shows the selected city: its cached forecast at once if there is one,
    /// then a fresh download.
    fn show_selected(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let city = self.places.borrow().selected().clone();
        let source = self.settings.borrow().source;
        self.shown_at.store(NOTHING_SHOWN, Ordering::Relaxed);
        ui.set_open_day(0); // Today open, also after switching city
        match self.dir.as_deref().and_then(|d| cache::load(d, &city, source, unix_now())) {
            Some(cached) => self.show(cached.forecast, cached.fetched_at),
            None => {
                *self.shown.borrow_mut() = None;
                show_loading(&ui, &city);
            }
        }
        self.update_city_list();
        self.update_source_list();
        self.refresh();
    }

    fn update_city_list(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let places = self.places.borrow();
        let items: Vec<CityItem> =
            places.cities().iter().enumerate().map(|(i, c)| city_item(c, i == places.selected_index())).collect();
        ui.set_cities(ModelRc::from(Rc::new(VecModel::from(items))));
    }

    /// Downloads the selected city's forecast in the background.
    fn refresh(&self) {
        let city = self.places.borrow().selected().clone();
        let source = self.settings.borrow().source;
        let generation = self.fetch_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let (weak, dir, shown_at, fetch_gen) =
            (self.ui.clone(), self.dir.clone(), self.shown_at.clone(), self.fetch_gen.clone());
        std::thread::spawn(move || {
            // The forecast, with several models, air quality and normals.
            let result = block_on(extras::download(dir.as_deref(), &city, source));
            let fetched_at = unix_now();
            if let (Ok(forecast), Some(dir)) = (&result, &dir)
                && let Err(e) = cache::save(dir, &city, source, forecast, fetched_at)
            {
                eprintln!("couldn't save the forecast cache: {e}");
            }
            #[cfg(target_os = "android")]
            if let Some(app) = ANDROID_APP.get() {
                android::notify_widgets(app);
            }
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                if fetch_gen.load(Ordering::Relaxed) != generation {
                    return; // the user switched city meanwhile
                }
                match result {
                    Ok(forecast) => {
                        ui.set_status(SharedString::new());
                        ui.set_refreshing(false);
                        APP.with(|a| {
                            if let Some(app) = a.get() {
                                app.show(forecast, fetched_at);
                            }
                        });
                    }
                    Err(e) => {
                        ui.set_refreshing(false);
                        match shown_at.load(Ordering::Relaxed) {
                            NOTHING_SHOWN => {
                                eprintln!("forecast download failed: {e}");
                                // Not "Loading…" any more: the message says what happened.
                                ui.set_condition(SharedString::new());
                                ui.set_error(lang().load_failed(e.problem()).into())
                            }
                            t => ui.set_status(lang().offline(unix_now() - t).into()),
                        }
                    }
                }
            });
        });
    }

    /// Finds the device's position and selects it as the "📍" city.
    /// `quiet`: no status messages and no permission dialog (startup refresh).
    fn locate(&self, quiet: bool) {
        let Some(ui) = self.ui.upgrade() else { return };
        if !quiet {
            ui.set_search_status(lang().finding_location().into());
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
        // Blocking JNI calls on a worker thread.
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
        ui.set_search_status(lang().searching().into());
        let (weak, search_gen, results) = (self.ui.clone(), self.search_gen.clone(), self.results.clone());
        std::thread::spawn(move || {
            std::thread::sleep(SEARCH_DELAY);
            if search_gen.load(Ordering::Relaxed) != generation {
                return; // another key was pressed
            }
            let found = block_on(search_cities(&text, lang()));
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                if search_gen.load(Ordering::Relaxed) != generation {
                    return;
                }
                let (status, places) = match found {
                    Ok(p) if p.is_empty() => (lang().no_city().to_string(), p),
                    Ok(p) => (String::new(), p),
                    Err(e) => {
                        eprintln!("city search failed: {e}");
                        (lang().search_failed(e.problem()).to_string(), Vec::new())
                    }
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
    use crate::android as loc;
    let app = ANDROID_APP.get().ok_or(lang().location_unavailable())?;
    if !loc::has_permission(app) {
        if quiet {
            return Err(String::new());
        }
        loc::request_permission(app);
        if !loc::wait_for_permission(app, Duration::from_secs(60)) {
            return Err(lang().permission_denied().into());
        }
    }
    let (lat, lon) = loc::current_position(app, Duration::from_secs(15)).ok_or(lang().no_fix())?;
    // About 1 km is plenty for the weather, and that's all that leaves the
    // phone (see docs/privacy-policy.md).
    let round = |x: f64| (x * 100.0).round() / 100.0;
    let (lat, lon) = (round(lat), round(lon));
    Ok(match loc::place_name(app, lat, lon) {
        Some(p) => Location::new(p.name, p.detail, lat, lon).with_areas(p.country_code, p.areas),
        None => Location::new(lang().my_location(), "", lat, lon),
    })
}

#[cfg(not(target_os = "android"))]
fn device_location(_quiet: bool) -> Result<Location, String> {
    Err(lang().location_unavailable().into())
}

/// Whether to offer "Use my location".
fn can_locate() -> bool {
    cfg!(target_os = "android")
}

/// Runs a future on a small single-threaded runtime (we're on a worker thread).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime").block_on(future)
}

fn city_item(c: &Location, selected: bool) -> CityItem {
    CityItem { name: c.name.as_str().into(), detail: c.detail.as_str().into(), selected, device: c.is_device }
}

/// A city with no cached forecast yet: its name and a loading state.
fn show_loading(ui: &MainWindow, city: &Location) {
    ui.set_error(SharedString::new());
    ui.set_status(SharedString::new());
    ui.set_city(city.name.as_str().into());
    ui.set_temperature("--°".into());
    ui.set_condition(lang().loading().into());
    ui.set_subline(SharedString::new());
    ui.set_details(Details::default());
    ui.set_updated(SharedString::new());
    ui.set_days(ModelRc::default());
}

fn apply(ui: &MainWindow, s: &MainScreen, colors: ColorMode) {
    animation::set_weather(s.scene);
    ui.set_vs_normal(s.vs_normal.as_str().into());
    ui.set_warnings(ModelRc::from(Rc::new(VecModel::from(
        s.warnings
            .iter()
            .map(|w| WarningItem {
                title: w.title.as_str().into(),
                detail: w.detail.as_str().into(),
                level: w.level.into(),
            })
            .collect::<Vec<_>>(),
    ))));
    ui.set_nowcast(Nowcast {
        summary: s.nowcast.summary.as_str().into(),
        wet: s.nowcast.wet,
        bars: ModelRc::from(Rc::new(VecModel::from(
            s.nowcast
                .bars
                .iter()
                .map(|b| NowBar { label: b.label.as_str().into(), height: b.height as f32, wet: b.wet })
                .collect::<Vec<_>>(),
        ))),
    });
    let a = &s.air;
    ui.set_air(AirInfo {
        aqi: a.aqi.as_str().into(),
        aqi_level: a.aqi_level.as_str().into(),
        pollen: a.pollen.as_str().into(),
        pollen_level: a.pollen_level.as_str().into(),
    });
    ui.set_error(SharedString::new());
    ui.set_city(s.city.as_str().into());
    ui.set_temperature(s.temperature.as_str().into());
    ui.set_condition(s.condition.as_str().into());
    ui.set_subline(format!("{} · {}", s.feels_like, s.high_low).into());
    ui.set_details(ui_details(&s.details));
    // Preview overrides (METEO_WEATHER / METEO_TIME, or debug.meteo.* on
    // Android), see animation.rs: redo the scene, palette and icon for them.
    let forced = match (animation::forced_condition(), animation::forced_phase()) {
        (None, None) => None,
        (condition, phase) => {
            let (condition, day) = condition.unwrap_or((meteo_core::Condition::Clear, true));
            let (phase, sun) =
                phase.unwrap_or((if day { meteo_core::Phase::Day } else { meteo_core::Phase::Night }, 0.5));
            let palette = meteo_core::Palette::for_weather(condition, phase);
            let mut land = meteo_core::landscape(condition, phase, sun, meteo_core::seed_for(&s.city));
            for (layer, color) in land.layers.iter_mut().zip(palette.hills()) {
                layer.color = color;
            }
            land.ground = palette.hills()[3];
            let icon = meteo_core::icon_kind(condition, phase != meteo_core::Phase::Night);
            Some((palette, land, icon))
        }
    };
    let (palette, land, icon) = match &forced {
        Some((p, l, i)) => (*p, l.clone(), *i),
        None => (s.palette, s.landscape.clone(), s.icon_kind.as_str()),
    };
    // Colours from the wallpaper instead: Material You roles, with the hills
    // redone in those tones.
    let (palette, land) = match colors {
        ColorMode::Wallpaper => {
            let p = wallpaper_palette(&palette);
            let mut land = land;
            for (layer, color) in land.layers.iter_mut().zip(p.hills()) {
                layer.color = color;
            }
            land.ground = p.hills()[3];
            (p, land)
        }
        ColorMode::Weather => (palette, land),
    };
    ui.set_theme(ui_theme(&palette));
    ui.set_icon_kind(icon.into());
    ui.set_ground(rgb(land.ground));
    let layers: Vec<LandLayer> =
        land.layers.iter().map(|l| LandLayer { path: l.path.as_str().into(), color: rgb(l.color) }).collect();
    ui.set_land_layers(ModelRc::from(Rc::new(VecModel::from(layers))));

    let days: Vec<DayItem> = s
        .days
        .iter()
        .map(|d| DayItem {
            label: d.label.as_str().into(),
            icon: d.icon.as_str().into(),
            icon_kind: d.icon_kind.as_str().into(),
            precipitation: d.precipitation.as_str().into(),
            min: d.min.as_str().into(),
            max: d.max.as_str().into(),
            bar_start: d.bar_start as f32,
            bar_end: d.bar_end as f32,
            rain_total: d.rain_total.as_str().into(),
            first_hour: d.first_hour as i32,
            rain_peak: d.rain_peak as f32,
            has_chance: d.has_chance,
            chart_width: d.chart.width as f32,
            chart_height: d.chart.height as f32,
            line_path: d.chart.line.as_str().into(),
            area_path: d.chart.area.as_str().into(),
            band_path: d.chart.band.as_str().into(),
            agreement: d.agreement.as_str().into(),
            has_range: d.has_range,
            hours: ModelRc::from(Rc::new(VecModel::from(
                d.hours
                    .iter()
                    .map(|h| DayHour {
                        label: h.label.as_str().into(),
                        temperature: h.temperature.as_str().into(),
                        icon: h.icon.as_str().into(),
                        icon_kind: h.icon_kind.as_str().into(),
                        precipitation: h.precipitation.as_str().into(),
                        rain: h.rain.as_str().into(),
                        rain_height: h.rain_height as f32,
                        rain_range: h.rain_range.as_str().into(),
                        range_low: h.range_low as f32,
                        range_high: h.range_high as f32,
                        is_now: h.is_now,
                        x: h.x as f32,
                        y: h.y as f32,
                    })
                    .collect::<Vec<_>>(),
            ))),
        })
        .collect();
    ui.set_days(ModelRc::from(Rc::new(VecModel::from(days))));
}

fn ui_details(d: &meteo_core::DetailsView) -> Details {
    Details {
        feels_like: d.feels_like.as_str().into(),
        feels_note: d.feels_note.as_str().into(),
        humidity: d.humidity.as_str().into(),
        dew_point: d.dew_point.as_str().into(),
        wind_speed: d.wind_speed.as_str().into(),
        wind_unit: d.wind_unit.as_str().into(),
        wind_gusts: d.wind_gusts.as_str().into(),
        wind_from: d.wind_from.as_str().into(),
        wind_arrow_path: d.wind_arrow_path.as_str().into(),
        sunrise: d.sunrise.as_str().into(),
        sunset: d.sunset.as_str().into(),
        daylight: d.daylight.as_str().into(),
        uv: d.uv.as_str().into(),
        uv_level: d.uv_level.as_str().into(),
        pressure: d.pressure.as_str().into(),
        pressure_note: d.pressure_note.as_str().into(),
    }
}

fn ui_labels(l: &meteo_core::Labels) -> Labels {
    Labels {
        forecast: l.forecast.into(),
        feels_like: l.feels_like.into(),
        humidity: l.humidity.into(),
        wind: l.wind.into(),
        sun: l.sun.into(),
        uv_index: l.uv_index.into(),
        pressure: l.pressure.into(),
        refresh: l.refresh.into(),
        data_credit: l.data_credit.into(),
        data_source: l.data_source.into(),
        data_source_help: l.data_source_help.into(),
        search: l.search.into(),
        done: l.done.into(),
        use_location: l.use_location.into(),
        saved_cities: l.saved_cities.into(),
        next_hours: l.next_hours.into(),
        air_quality: l.air_quality.into(),
        pollen: l.pollen.into(),
        settings: l.settings.into(),
        units: l.units.into(),
        metric: l.metric.into(),
        imperial: l.imperial.into(),
        colors: l.colors.into(),
        colors_weather: l.colors_weather.into(),
        colors_wallpaper: l.colors_wallpaper.into(),
        language: l.language.into(),
        language_system: l.language_system.into(),
        notifications: l.notifications.into(),
        morning_summary: l.morning_summary.into(),
        morning_summary_help: l.morning_summary_help.into(),
        rain_alerts: l.rain_alerts.into(),
        rain_alerts_help: l.rain_alerts_help.into(),
        warning_alerts: l.warning_alerts.into(),
        warning_alerts_help: l.warning_alerts_help.into(),
        warnings_source: l.warnings_source.into(),
    }
}

/// Material You's roles (from the wallpaper) as a palette; rain/snow and
/// clouds keep the weather's light or dark look.
fn wallpaper_palette(weather: &Palette) -> Palette {
    static SCHEME: OnceLock<theme::Scheme> = OnceLock::new();
    let t = SCHEME.get_or_init(theme::system);
    let dark = (t.surface >> 16 & 0xFF) + (t.surface >> 8 & 0xFF) + (t.surface & 0xFF) < 3 * 128;
    Palette {
        top: t.primary_container,
        bottom: t.surface,
        card: t.surface_container,
        card_strong: t.surface_container_high,
        ink: t.on_surface,
        ink_soft: t.on_surface_variant,
        accent: t.primary,
        on_accent: t.on_primary,
        particle: if dark { 0xFFFFFF } else { t.primary },
        cloud: if dark { weather.cloud } else { t.surface_container },
        dark,
    }
}

/// The weather palette as the UI's colour roles.
fn ui_theme(p: &meteo_core::Palette) -> Theme {
    Theme {
        primary: rgb(p.accent),
        on_primary: rgb(p.on_accent),
        primary_container: rgb(p.card_strong),
        on_primary_container: rgb(p.ink),
        secondary_container: rgb(p.card_strong),
        on_secondary_container: rgb(p.ink),
        tertiary_container: rgb(p.card_strong),
        on_tertiary_container: rgb(p.ink),
        surface: rgb(p.bottom),
        surface_container: rgb(p.card),
        surface_container_high: rgb(p.card_strong),
        on_surface: rgb(p.ink),
        on_surface_variant: rgb(p.ink_soft),
        outline_variant: rgb(meteo_core::mix(p.card, p.ink, 0.18)),
        bg_top: rgb(p.top),
        bg_bottom: rgb(p.bottom),
        particle: rgb(p.particle),
        cloud: rgb(p.cloud),
    }
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
    // In the background: no sky animation, no compass sensor.
    slint::android::init_with_event_listener(app, |event| {
        use slint::android::android_activity::{MainEvent, PollEvent};
        let running = match event {
            PollEvent::Main(MainEvent::Pause) => false,
            PollEvent::Main(MainEvent::Resume { .. }) => true,
            _ => return,
        };
        animation::set_running(running);
        android::set_compass_active(running);
    })
    .expect("Slint Android backend");
    run().expect("Slint event loop");
}
