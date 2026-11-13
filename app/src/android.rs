//! Android platform calls through JNI (no Java/Kotlin code needed): the
//! device's position (`LocationManager`, `Geocoder`) and widget refreshes.
//!
//! A NativeActivity can't receive callbacks (permission results, location
//! listeners) without a Java class, so this polls instead: it polls
//! `checkSelfPermission` after asking, and it starts location updates
//! delivered to a `PendingIntent` nobody listens to (enough to make the
//! provider produce a fix) while polling `getLastKnownLocation`.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use slint::android::AndroidApp;

const PERMISSIONS: [&str; 2] = [
    "android.permission.ACCESS_COARSE_LOCATION",
    "android.permission.ACCESS_FINE_LOCATION",
];
/// Most precise first. "fused" exists from Android 12; unknown providers are skipped.
const PROVIDERS: [&str; 3] = ["fused", "gps", "network"];
/// A last known position this recent is used straight away.
const FRESH_ENOUGH: Duration = Duration::from_secs(15 * 60);

type Result<T> = std::result::Result<T, jni::errors::Error>;

/// Runs `f` with a JNI environment for this thread and the activity.
fn with_activity<T>(app: &AndroidApp, f: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T>) -> Result<T> {
    // SAFETY: android-activity owns valid JavaVM and activity pointers for the app's lifetime.
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast())? };
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let mut env = vm.attach_current_thread()?;
    // A local frame frees every local reference made inside `f`.
    let result = env.with_local_frame(64, |env| f(env, &activity));
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result
}

/// Clears a pending Java exception so later JNI calls keep working.
fn ok_or_clear<T>(env: &mut JNIEnv, r: Result<T>) -> Option<T> {
    if r.is_err() {
        let _ = env.exception_clear();
    }
    r.ok()
}

pub fn has_permission(app: &AndroidApp) -> bool {
    with_activity(app, |env, activity| {
        let perm = env.new_string(PERMISSIONS[0])?;
        let state = env
            .call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?
            .i()?;
        Ok(state == 0) // PackageManager.PERMISSION_GRANTED
    })
    .unwrap_or(false)
}

/// Shows the system permission dialog.
pub fn request_permission(app: &AndroidApp) {
    let _ = with_activity(app, |env, activity| {
        let array = env.new_object_array(PERMISSIONS.len() as i32, "java/lang/String", JObject::null())?;
        for (i, p) in PERMISSIONS.iter().enumerate() {
            let s = env.new_string(p)?;
            env.set_object_array_element(&array, i as i32, s)?;
        }
        env.call_method(activity, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&array), JValue::Int(1)])?;
        Ok(())
    });
}

/// Waits up to `timeout` for the user to answer the permission dialog.
pub fn wait_for_permission(app: &AndroidApp, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if has_permission(app) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    false
}

/// The device's latitude and longitude: a recent last known position, or
/// a fresh fix within `timeout`, or failing that the last known one however old.
pub fn current_position(app: &AndroidApp, timeout: Duration) -> Option<(f64, f64)> {
    with_activity(app, |env, activity| {
        let service = env.new_string("location")?;
        let manager = env
            .call_method(activity, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&service)])?
            .l()?;

        if let Some(fix) = last_known(env, &manager).filter(|f| f.age < FRESH_ENOUGH) {
            return Ok(Some((fix.latitude, fix.longitude)));
        }

        // Ask the providers for updates so they produce a fresh fix.
        let action = env.new_string("dev.meteo.app.LOCATION")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        const FLAG_UPDATE_CURRENT: i32 = 0x0800_0000;
        const FLAG_IMMUTABLE: i32 = 0x0400_0000;
        let pending = env
            .call_static_method(
                "android/app/PendingIntent",
                "getBroadcast",
                "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
                &[JValue::Object(activity), JValue::Int(0), JValue::Object(&intent), JValue::Int(FLAG_UPDATE_CURRENT | FLAG_IMMUTABLE)],
            )?
            .l()?;
        for provider in PROVIDERS {
            let name = env.new_string(provider)?;
            let r = env.call_method(
                &manager,
                "requestLocationUpdates",
                "(Ljava/lang/String;JFLandroid/app/PendingIntent;)V",
                &[JValue::Object(&name), JValue::Long(1000), JValue::Float(0.0), JValue::Object(&pending)],
            );
            ok_or_clear(env, r);
        }

        let start = Instant::now();
        let mut found = None;
        while start.elapsed() < timeout {
            if let Some(fix) = last_known(env, &manager).filter(|f| f.age < Duration::from_secs(60)) {
                found = Some(fix);
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let r = env.call_method(&manager, "removeUpdates", "(Landroid/app/PendingIntent;)V", &[JValue::Object(&pending)]);
        ok_or_clear(env, r);

        Ok(found.or_else(|| last_known(env, &manager)).map(|f| (f.latitude, f.longitude)))
    })
    .ok()
    .flatten()
}

struct Fix {
    latitude: f64,
    longitude: f64,
    age: Duration,
}

/// The most recent last known position across providers.
fn last_known(env: &mut JNIEnv, manager: &JObject) -> Option<Fix> {
    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    let mut best: Option<Fix> = None;
    for provider in PROVIDERS {
        let Ok(name) = env.new_string(provider) else { continue };
        let r = env
            .call_method(manager, "getLastKnownLocation", "(Ljava/lang/String;)Landroid/location/Location;", &[JValue::Object(&name)])
            .and_then(|v| v.l());
        let Some(loc) = ok_or_clear(env, r).filter(|l| !l.is_null()) else { continue };
        let read = |env: &mut JNIEnv, m: &str| env.call_method(&loc, m, "()D", &[]).and_then(|v| v.d());
        let (Ok(latitude), Ok(longitude)) = (read(env, "getLatitude"), read(env, "getLongitude")) else {
            let _ = env.exception_clear();
            continue;
        };
        let time = env.call_method(&loc, "getTime", "()J", &[]).and_then(|v| v.j()).unwrap_or(0);
        let age = Duration::from_millis(now_ms.saturating_sub(time).max(0) as u64);
        if best.as_ref().is_none_or(|b| age < b.age) {
            best = Some(Fix { latitude, longitude, age });
        }
    }
    best
}

/// Town name and "region, country" for a position, from Android's Geocoder.
pub fn place_name(app: &AndroidApp, latitude: f64, longitude: f64) -> Option<(String, String)> {
    with_activity(app, |env, activity| {
        let geocoder = env.new_object("android/location/Geocoder", "(Landroid/content/Context;)V", &[JValue::Object(activity)])?;
        let list = env
            .call_method(&geocoder, "getFromLocation", "(DDI)Ljava/util/List;", &[JValue::Double(latitude), JValue::Double(longitude), JValue::Int(1)])?
            .l()?;
        if list.is_null() || env.call_method(&list, "size", "()I", &[])?.i()? == 0 {
            return Ok(None);
        }
        let address = env.call_method(&list, "get", "(I)Ljava/lang/Object;", &[JValue::Int(0)])?.l()?;
        let mut field = |name: &str| -> Result<Option<String>> {
            let s = env.call_method(&address, name, "()Ljava/lang/String;", &[])?.l()?;
            if s.is_null() {
                return Ok(None);
            }
            Ok(Some(env.get_string(&JString::from(s))?.into()))
        };
        let town = field("getLocality")?.or(field("getSubAdminArea")?);
        let region = field("getAdminArea")?;
        let country = field("getCountryName")?;
        let detail = [region, country].into_iter().flatten().collect::<Vec<_>>().join(", ");
        Ok(town.map(|t| (t, detail)))
    })
    .ok()
    .flatten()
}

/// Asks the home-screen widget (if any is placed) to redraw from the cache.
/// Called after the app saved a fresh forecast or switched city.
pub fn notify_widgets(app: &AndroidApp) {
    let _ = with_activity(app, |env, activity| {
        let manager = env
            .call_static_method(
                "android/appwidget/AppWidgetManager",
                "getInstance",
                "(Landroid/content/Context;)Landroid/appwidget/AppWidgetManager;",
                &[JValue::Object(activity)],
            )?
            .l()?;
        let class = env.new_string("dev.meteo.app.widget.MeteoWidgetReceiver")?;
        let component = env.new_object(
            "android/content/ComponentName",
            "(Landroid/content/Context;Ljava/lang/String;)V",
            &[JValue::Object(activity), JValue::Object(&class)],
        )?;
        let ids = env
            .call_method(&manager, "getAppWidgetIds", "(Landroid/content/ComponentName;)[I", &[JValue::Object(&component)])?
            .l()?;
        if env.get_array_length(&jni::objects::JIntArray::from(env.new_local_ref(&ids)?))? == 0 {
            return Ok(()); // no widget on the home screen
        }
        let action = env.new_string("android.appwidget.action.APPWIDGET_UPDATE")?;
        let intent = env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
        env.call_method(&intent, "setComponent", "(Landroid/content/ComponentName;)Landroid/content/Intent;", &[JValue::Object(&component)])?;
        let key = env.new_string("appWidgetIds")?;
        env.call_method(&intent, "putExtra", "(Ljava/lang/String;[I)Landroid/content/Intent;", &[JValue::Object(&key), JValue::Object(&ids)])?;
        env.call_method(activity, "sendBroadcast", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    });
}

/// Material You's tonal palettes (Android 12+), from the wallpaper:
/// `tone("accent1", 600)` reads `android.R.color.system_accent1_600`.
/// Also says whether the system is in dark mode.
pub struct SystemColors {
    tones: std::collections::HashMap<String, u32>,
    pub dark: bool,
}

impl SystemColors {
    /// 0xRRGGBB of e.g. ("neutral2", 100).
    pub fn tone(&self, palette: &str, tone: u32) -> Option<u32> {
        self.tones.get(&format!("{palette}_{tone}")).copied()
    }
}

const PALETTES: [&str; 5] = ["accent1", "accent2", "accent3", "neutral1", "neutral2"];
const TONES: [u32; 13] = [0, 10, 50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000];

/// Dark mode, plus the dynamic palettes on Android 12+ (empty before).
pub fn system_colors(app: &AndroidApp) -> Option<SystemColors> {
    with_activity(app, |env, activity| {
        // Configuration.uiMode & UI_MODE_NIGHT_MASK == UI_MODE_NIGHT_YES
        let resources = env.call_method(activity, "getResources", "()Landroid/content/res/Resources;", &[])?.l()?;
        let config = env
            .call_method(&resources, "getConfiguration", "()Landroid/content/res/Configuration;", &[])?
            .l()?;
        let ui_mode = env.get_field(&config, "uiMode", "I")?.i()?;
        let dark = ui_mode & 0x30 == 0x20;

        let mut tones = std::collections::HashMap::new();
        let sdk = env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()?;
        if sdk >= 31 {
            for palette in PALETTES {
                for tone in TONES {
                    let name = format!("system_{palette}_{tone}");
                    let id = env.get_static_field("android/R$color", name.as_str(), "I")?.i()?;
                    let argb = env.call_method(activity, "getColor", "(I)I", &[JValue::Int(id)])?.i()?;
                    tones.insert(format!("{palette}_{tone}"), argb as u32 & 0xFF_FFFF);
                }
            }
        }
        Ok(SystemColors { tones, dark })
    })
    .ok()
}

/// The app's language, e.g. "fr" (from its configuration, so a per-app
/// language chosen in Android's settings counts too).
pub fn language(app: &AndroidApp) -> Option<String> {
    with_activity(app, |env, activity| {
        let resources = env.call_method(activity, "getResources", "()Landroid/content/res/Resources;", &[])?.l()?;
        let config = env
            .call_method(&resources, "getConfiguration", "()Landroid/content/res/Configuration;", &[])?
            .l()?;
        let locales = env.call_method(&config, "getLocales", "()Landroid/os/LocaleList;", &[])?.l()?;
        let locale = env.call_method(&locales, "get", "(I)Ljava/util/Locale;", &[JValue::Int(0)])?.l()?;
        let lang = env.call_method(&locale, "getLanguage", "()Ljava/lang/String;", &[])?.l()?;
        Ok(env.get_string(&JString::from(lang))?.into())
    })
    .ok()
}

/// Asks for permission to post notifications (Android 13+; granted
/// automatically before that).
pub fn request_notification_permission(app: &AndroidApp) {
    let _ = with_activity(app, |env, activity| {
        let sdk = env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?.i()?;
        if sdk < 33 {
            return Ok(());
        }
        let array = env.new_object_array(1, "java/lang/String", JObject::null())?;
        let perm = env.new_string("android.permission.POST_NOTIFICATIONS")?;
        env.set_object_array_element(&array, 0, perm)?;
        env.call_method(activity, "requestPermissions", "([Ljava/lang/String;I)V", &[JValue::Object(&array), JValue::Int(2)])?;
        Ok(())
    });
}
