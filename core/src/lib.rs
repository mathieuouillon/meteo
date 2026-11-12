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
mod geocode;
mod model;
mod places;
mod scene;
mod source;
mod view;
mod widget;

pub use api::{Error, fetch_forecast, parse_forecast};
pub use cache::{CachedForecast, unix_now};
pub use chart::{ChartGeometry, ChartPoint, temperature_chart};
pub use condition::{Condition, Sky};
pub use geocode::{parse_places, search_cities};
pub use model::{Current, Day, Forecast, Hour, Location};
pub use places::Places;
pub use source::{Settings, Source};
pub use scene::{CLOUD_HEIGHT, CLOUD_WIDTH, CloudView, FogView, Scene, SceneFrame, SceneSpec};
pub use view::{CHART_HEIGHT, DayHourView, DayView, HOUR_STEP, HOURS_SHOWN, HourView, MainScreen, rain_amount, temp, updated_ago};
pub use widget::{WidgetHour, WidgetState, WidgetView, refresh_selected, widget_state};
