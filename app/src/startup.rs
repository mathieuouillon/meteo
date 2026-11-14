//! How long the app takes to start: one line in the system log (logcat tag
//! "Meteo") with the time from the start of the Rust code to the window, the
//! cached forecast on screen and the first frame drawn.

use std::fmt::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();
static MARKS: Mutex<String> = Mutex::new(String::new());

pub fn begin() {
    let _ = START.get_or_init(Instant::now);
}

pub fn mark(what: &str) {
    let Some(start) = START.get() else { return };
    if let Ok(mut marks) = MARKS.lock() {
        let sep = if marks.is_empty() { "" } else { ", " };
        let _ = write!(marks, "{sep}{what} {} ms", start.elapsed().as_millis());
    }
}

/// Marks the first frame and writes the line.
pub fn after_first_frame(ui: &crate::MainWindow) {
    use slint::ComponentHandle;
    let mut done = false;
    let weak = ui.as_weak();
    let _ = ui.window().set_rendering_notifier(move |state, _| {
        if done {
            return;
        }
        match state {
            slint::RenderingState::RenderingSetup => return mark("renderer ready"),
            slint::RenderingState::BeforeRendering => return mark("drawing"),
            slint::RenderingState::AfterRendering => {}
            _ => return,
        }
        done = true;
        mark("first frame");
        if let Some(ui) = weak.upgrade() {
            ui.set_settled(true);
        }
        if let Ok(marks) = MARKS.lock() {
            log(&format!("startup: {marks}"));
        }
    });
}

#[cfg(target_os = "android")]
fn log(text: &str) {
    use std::ffi::{CString, c_char, c_int};
    #[link(name = "log")]
    unsafe extern "C" {
        fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
    }
    const INFO: c_int = 4;
    if let Ok(text) = CString::new(text) {
        // SAFETY: both pointers are valid NUL-terminated strings for the call.
        unsafe { __android_log_write(INFO, c"Meteo".as_ptr(), text.as_ptr()) };
    }
}

#[cfg(not(target_os = "android"))]
fn log(text: &str) {
    eprintln!("{text}");
}
