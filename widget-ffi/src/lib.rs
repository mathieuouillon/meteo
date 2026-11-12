//! JNI entry points for `dev.meteo.app.widget.RustBridge` (Kotlin).
//!
//! The Android widget runs without the Slint UI, so it loads this small
//! library (`libmeteo_widget.so`) instead of `libmeteo.so`. Data goes back
//! to Kotlin as JSON (`meteo_core::WidgetState`).

use std::path::PathBuf;

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jstring};

fn data_dir(env: &mut JNIEnv, dir: &JString) -> Option<PathBuf> {
    env.get_string(dir).ok().map(|s| PathBuf::from(String::from(s)))
}

/// `external fun state(dataDir: String, language: String): String` — the
/// widget's content as JSON, in that language ("fr", "en", …).
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_meteo_app_widget_RustBridge_state<'l>(
    mut env: JNIEnv<'l>,
    _class: JClass<'l>,
    dir: JString<'l>,
    language: JString<'l>,
) -> jstring {
    let lang = env
        .get_string(&language)
        .map(|s| meteo_core::Lang::from_code(&String::from(s)))
        .unwrap_or_default();
    let state = data_dir(&mut env, &dir).map(|d| meteo_core::widget_state(&d, meteo_core::unix_now(), lang));
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
