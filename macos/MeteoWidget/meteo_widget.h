// C API of the Rust widget-ffi crate (libmeteo_widget.a).
#include <stdbool.h>
#include <stdint.h>

/// The widget's content at `unix_now`, as JSON (meteo_core::WidgetState).
/// Free with meteo_string_free.
char *meteo_widget_state(const char *data_dir, int64_t unix_now);

/// Downloads the selected city's forecast into the cache. Blocking.
bool meteo_widget_refresh(const char *data_dir);

void meteo_string_free(char *s);
