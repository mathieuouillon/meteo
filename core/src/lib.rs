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
mod model;
mod view;

pub use api::{Error, fetch_forecast, parse_forecast};
pub use cache::{CachedForecast, unix_now};
pub use chart::{ChartGeometry, ChartPoint, temperature_chart};
pub use condition::{Condition, Sky};
pub use model::{Current, Day, Forecast, Hour, Location};
pub use view::{CHART_HEIGHT, DayView, HOUR_STEP, HOURS_SHOWN, HourView, MainScreen, temp, updated_ago};
