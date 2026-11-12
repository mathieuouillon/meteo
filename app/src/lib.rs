use std::rc::Rc;

use meteo_core::{Location, MainScreen, fetch_forecast};
use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

/// Builds the window, starts the first fetch and runs the event loop.
pub fn run() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    let weak = ui.as_weak();
    let load = move || {
        let weak = weak.clone();
        // Network work happens off the UI thread; results are posted back.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            let result = rt.block_on(fetch_forecast(&Location::paris()));
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                match result {
                    Ok(forecast) => apply(&ui, &MainScreen::from_forecast(&forecast)),
                    Err(e) => ui.set_error(format!("Couldn't load the forecast: {e}").into()),
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
    slint::android::init(app).expect("Slint Android backend");
    run().expect("Slint event loop");
}
