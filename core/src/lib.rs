//! Shared weather logic used by every front-end (Slint, Tauri, widgets).
//!
//! Everything a UI needs is computed here: fetching and parsing Open-Meteo,
//! mapping WMO weather codes to conditions and colours, and even the geometry
//! of the hourly temperature curve, so each UI only has to draw it.

mod api;
pub mod cache;
mod chart;
mod condition;
mod date;
pub mod extras;
mod geocode;
mod i18n;
mod landscape;
mod model;
pub mod notify;
mod palette;
mod places;
mod scene;
mod source;
mod units;
mod view;
mod widget;

pub use api::{Error, Problem, fetch_forecast, parse_forecast};
pub use cache::{CachedForecast, unix_now};
pub use chart::{ChartGeometry, ChartPoint, temperature_chart};
pub use condition::{Condition, Sky};
pub use geocode::{parse_places, search_cities};
pub use i18n::{Labels, Lang};
pub use landscape::{LANDSCAPE_HEIGHT, LANDSCAPE_WIDTH, Landscape, LandscapeLayer, Phase, landscape, mix, seed_for};
pub use model::{Current, Day, Forecast, Hour, Location, Quarter};
pub use palette::{Palette, icon_kind};
pub use places::Places;
pub use scene::{CLOUD_HEIGHT, CLOUD_WIDTH, CloudView, FogView, SURFACE_MARGIN, Scene, SceneFrame, SceneSpec};
pub use source::{ColorMode, LanguageChoice, Settings, Source};
pub use units::Units;
pub use view::{
    CHART_HEIGHT, DayHourView, DayView, DetailsView, HOUR_STEP, HOURS_SHOWN, HourView, MainScreen, rain_amount, temp,
};
pub use widget::{WidgetHour, WidgetState, WidgetView, refresh_selected, widget_state};
