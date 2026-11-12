//! Entry points for the home-screen widgets, which run without the Slint UI:
//!
//! - Android: JNI functions for `dev.meteo.app.widget.RustBridge` (Kotlin),
//!   in `libmeteo_widget.so`.
//! - macOS: C functions (see `macos/MeteoWidget/meteo_widget.h`) for the
//!   Swift WidgetKit extension, linked from `libmeteo_widget.a`.
//!
//! Both return `meteo_core::WidgetState` as JSON.

use std::path::PathBuf;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jstring};

fn data_dir(env: &mut JNIEnv, dir: &JString) -> Option<PathBuf> {
    env.get_string(dir).ok().map(|s| PathBuf::from(String::from(s)))
}

/// `external fun state(dataDir: String): String` — the widget's content as JSON.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_meteo_app_widget_RustBridge_state<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    dir: JString<'l>,
) -> jstring {
    let state = data_dir(&mut env, &dir).map(|d| meteo_core::widget_state(&d, meteo_core::unix_now()));
    let json = state
        .and_then(|s| serde_json::to_string(&s).ok())
        .unwrap_or_else(|| r#"{"needs_refresh":true,"view":null}"#.into());
    env.new_string(json).map_or(std::ptr::null_mut(), |s| s.into_raw())
}

/// `external fun refresh(dataDir: String): Boolean` — downloads the selected
/// city's forecast into the cache. Blocking: call it off the main thread.
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_meteo_app_widget_RustBridge_refresh<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    dir: JString<'l>,
) -> jboolean {
    let Some(dir) = data_dir(&mut env, &dir) else { return JNI_FALSE };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        return JNI_FALSE;
    };
    match rt.block_on(meteo_core::refresh_selected(&dir)) {
        Ok(()) => JNI_TRUE,
        Err(_) => JNI_FALSE,
    }
}

// ── C API (macOS widget) ─────────────────────────────────────────────────

use std::ffi::{CStr, CString, c_char};

fn c_dir(dir: *const c_char) -> Option<PathBuf> {
    if dir.is_null() {
        return None;
    }
    // SAFETY: the caller passes a valid NUL-terminated string.
    let s = unsafe { CStr::from_ptr(dir) }.to_str().ok()?;
    Some(PathBuf::from(s))
}

/// The widget's content at `unix_now` as JSON. Free it with `meteo_string_free`.
#[unsafe(no_mangle)]
pub extern "C" fn meteo_widget_state(dir: *const c_char, unix_now: i64) -> *mut c_char {
    let json = c_dir(dir)
        .map(|d| meteo_core::widget_state(&d, unix_now))
        .and_then(|s| serde_json::to_string(&s).ok())
        .unwrap_or_else(|| r#"{"needs_refresh":true,"view":null}"#.into());
    CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// Downloads the selected city's forecast into the cache. Blocking.
#[unsafe(no_mangle)]
pub extern "C" fn meteo_widget_refresh(dir: *const c_char) -> bool {
    let Some(dir) = c_dir(dir) else { return false };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
        return false;
    };
    rt.block_on(meteo_core::refresh_selected(&dir)).is_ok()
}

/// Frees a string returned by this library.
#[unsafe(no_mangle)]
pub extern "C" fn meteo_string_free(s: *mut c_char) {
    if !s.is_null() {
        // SAFETY: `s` came from `CString::into_raw` above.
        drop(unsafe { CString::from_raw(s) });
    }
}
