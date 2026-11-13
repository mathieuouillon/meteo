//! Android platform calls through JNI (no Java/Kotlin code needed): the
//! device's position (`LocationManager`, `Geocoder`) and widget refreshes.
//!
//! A NativeActivity can't receive callbacks (permission results, location
//! listeners) without a Java class, so this polls instead: it polls
//! `checkSelfPermission` after asking, and it starts location updates
//! delivered to a `PendingIntent` nobody listens to (enough to make the
//! provider produce a fix) while polling `getLastKnownLocation`.

use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use jni::objects::{JObject, JString, JValue};
use jni::{JNIEnv, JavaVM};
use slint::android::AndroidApp;

/// Approximate location is plenty for the weather (and what Android's
/// guidelines ask an app like this to request).
const PERMISSIONS: [&str; 1] = ["android.permission.ACCESS_COARSE_LOCATION"];
/// Providers that work with approximate location. "fused" exists from
/// Android 12; unknown providers are skipped.
const PROVIDERS: [&str; 2] = ["fused", "network"];
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
        let state =
            env.call_method(activity, "checkSelfPermission", "(Ljava/lang/String;)I", &[JValue::Object(&perm)])?.i()?;
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
        env.call_method(
            activity,
            "requestPermissions",
            "([Ljava/lang/String;I)V",
            &[JValue::Object(&array), JValue::Int(1)],
        )?;
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
            .call_method(
                activity,
                "getSystemService",
                "(Ljava/lang/String;)Ljava/lang/Object;",
                &[JValue::Object(&service)],
            )?
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
                &[
                    JValue::Object(activity),
                    JValue::Int(0),
                    JValue::Object(&intent),
                    JValue::Int(FLAG_UPDATE_CURRENT | FLAG_IMMUTABLE),
                ],
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
        let r =
            env.call_method(&manager, "removeUpdates", "(Landroid/app/PendingIntent;)V", &[JValue::Object(&pending)]);
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
            .call_method(
                manager,
                "getLastKnownLocation",
                "(Ljava/lang/String;)Landroid/location/Location;",
                &[JValue::Object(&name)],
            )
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
/// A place named by Android's geocoder.
pub struct PlaceName {
    pub name: String,
    /// "Region, Country".
    pub detail: String,
    pub country_code: String,
    /// Region, sub-region (département…), town: for weather warnings.
    pub areas: Vec<String>,
}

pub fn place_name(app: &AndroidApp, latitude: f64, longitude: f64) -> Option<PlaceName> {
    with_activity(app, |env, activity| {
        let geocoder =
            env.new_object("android/location/Geocoder", "(Landroid/content/Context;)V", &[JValue::Object(activity)])?;
        let list = env
            .call_method(
                &geocoder,
                "getFromLocation",
                "(DDI)Ljava/util/List;",
                &[JValue::Double(latitude), JValue::Double(longitude), JValue::Int(1)],
            )?
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
        let locality = field("getLocality")?;
        let sub_region = field("getSubAdminArea")?;
        let region = field("getAdminArea")?;
        let country = field("getCountryName")?;
        let country_code = field("getCountryCode")?.unwrap_or_default();
        let town = locality.clone().or(sub_region.clone());
        let detail = [region.clone(), country].into_iter().flatten().collect::<Vec<_>>().join(", ");
        let areas = [region, sub_region, locality].into_iter().flatten().collect();
        Ok(town.map(|name| PlaceName { name, detail, country_code, areas }))
    })
    .ok()
    .flatten()
}

/// The home-screen widgets (Kotlin Glance receivers).
const WIDGET_RECEIVERS: [&str; 3] = [
    "dev.meteo.app.widget.MeteoWidgetReceiver",
    "dev.meteo.app.widget.RainWidgetReceiver",
    "dev.meteo.app.widget.RoundWidgetReceiver",
];

/// Asks the home-screen widgets (those placed) to redraw from the cache.
/// Called after the app saved a fresh forecast or switched city.
pub fn notify_widgets(app: &AndroidApp) {
    for receiver in WIDGET_RECEIVERS {
        let _ = with_activity(app, |env, activity| {
            let manager = env
                .call_static_method(
                    "android/appwidget/AppWidgetManager",
                    "getInstance",
                    "(Landroid/content/Context;)Landroid/appwidget/AppWidgetManager;",
                    &[JValue::Object(activity)],
                )?
                .l()?;
            let class = env.new_string(receiver)?;
            let component = env.new_object(
                "android/content/ComponentName",
                "(Landroid/content/Context;Ljava/lang/String;)V",
                &[JValue::Object(activity), JValue::Object(&class)],
            )?;
            let ids = env
                .call_method(
                    &manager,
                    "getAppWidgetIds",
                    "(Landroid/content/ComponentName;)[I",
                    &[JValue::Object(&component)],
                )?
                .l()?;
            if env.get_array_length(&jni::objects::JIntArray::from(env.new_local_ref(&ids)?))? == 0 {
                return Ok(()); // none of these on the home screen
            }
            let action = env.new_string("android.appwidget.action.APPWIDGET_UPDATE")?;
            let intent =
                env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[JValue::Object(&action)])?;
            env.call_method(
                &intent,
                "setComponent",
                "(Landroid/content/ComponentName;)Landroid/content/Intent;",
                &[JValue::Object(&component)],
            )?;
            let key = env.new_string("appWidgetIds")?;
            env.call_method(
                &intent,
                "putExtra",
                "(Ljava/lang/String;[I)Landroid/content/Intent;",
                &[JValue::Object(&key), JValue::Object(&ids)],
            )?;
            env.call_method(activity, "sendBroadcast", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
            Ok(())
        });
    }
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
        let config =
            env.call_method(&resources, "getConfiguration", "()Landroid/content/res/Configuration;", &[])?.l()?;
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
        let config =
            env.call_method(&resources, "getConfiguration", "()Landroid/content/res/Configuration;", &[])?.l()?;
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
        env.call_method(
            activity,
            "requestPermissions",
            "([Ljava/lang/String;I)V",
            &[JValue::Object(&array), JValue::Int(2)],
        )?;
        Ok(())
    });
}

/// Whether the compass should read its sensor (false while the app is in
/// the background), and the compass thread's looper, to wake it on a change.
static COMPASS_WANTED: AtomicBool = AtomicBool::new(true);
static COMPASS_LOOPER: AtomicPtr<ndk_sys::ALooper> = AtomicPtr::new(std::ptr::null_mut());

/// Turns the compass sensor off while the app is in the background, and on
/// again when it comes back.
pub fn set_compass_active(active: bool) {
    COMPASS_WANTED.store(active, Ordering::Relaxed);
    let looper = COMPASS_LOOPER.load(Ordering::Acquire);
    if !looper.is_null() {
        // SAFETY: the looper belongs to the compass thread, which never exits.
        unsafe { ndk_sys::ALooper_wake(looper) };
    }
}

/// The phone's heading for the wind compass: a thread reading the rotation
/// vector sensor (NDK sensor API, no Java) about 15 times a second and
/// calling `on_heading` with degrees clockwise from north (portrait).
/// Does nothing on devices without that sensor. The sensor is only on while
/// the app is in front (`set_compass_active`).
pub fn start_compass(on_heading: impl Fn(f32) + Send + 'static) {
    std::thread::spawn(move || {
        use ndk_sys::*;
        // SAFETY: plain NDK calls on this thread's own looper; the queue and
        // sensor stay valid for the thread's (= the process's) lifetime.
        unsafe {
            // Events without a callback are only delivered with this option.
            let looper = ALooper_prepare(ALOOPER_PREPARE_ALLOW_NON_CALLBACKS as i32);
            let manager = ASensorManager_getInstanceForPackage(c"dev.meteo.app".as_ptr());
            if manager.is_null() {
                return;
            }
            let sensor = ASensorManager_getDefaultSensor(manager, ASENSOR_TYPE_ROTATION_VECTOR as i32);
            if sensor.is_null() {
                return;
            }
            let queue = ASensorManager_createEventQueue(manager, looper, 1, None, std::ptr::null_mut());
            if queue.is_null() {
                return;
            }
            COMPASS_LOOPER.store(looper, Ordering::Release);
            let mut enabled = false;
            let mut events: [ASensorEvent; 8] = std::mem::zeroed();
            loop {
                // Follow the app going to the background and back.
                let wanted = COMPASS_WANTED.load(Ordering::Relaxed);
                if wanted && !enabled {
                    enabled = ASensorEventQueue_enableSensor(queue, sensor) >= 0;
                    if enabled {
                        ASensorEventQueue_setEventRate(queue, sensor, 66_000);
                    }
                } else if !wanted && enabled {
                    ASensorEventQueue_disableSensor(queue, sensor);
                    enabled = false;
                }
                // Sleeps until a reading arrives, or `set_compass_active` wakes it.
                ALooper_pollOnce(-1, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
                if !enabled {
                    continue;
                }
                let n = ASensorEventQueue_getEvents(queue, events.as_mut_ptr(), events.len());
                if n <= 0 {
                    continue;
                }
                let e = &events[n as usize - 1];
                let v = e.__bindgen_anon_1.__bindgen_anon_1.data;
                on_heading(heading_from_rotation_vector(v[0], v[1], v[2]));
            }
        }
    });
}

/// Azimuth from a rotation vector (x, y, z = axis × sin(θ/2)), as Android's
/// SensorManager.getRotationMatrixFromVector + getOrientation compute it.
fn heading_from_rotation_vector(x: f32, y: f32, z: f32) -> f32 {
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    let r1 = 2.0 * (x * y - z * w);
    let r4 = 1.0 - 2.0 * (x * x + z * z);
    (r1.atan2(r4).to_degrees() + 360.0) % 360.0
}

/// Opens `url` in the browser (an ACTION_VIEW intent).
pub fn open_url(app: &AndroidApp, url: &str) {
    let _ = with_activity(app, |env, activity| {
        let s = env.new_string(url)?;
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[JValue::Object(&s)],
            )?
            .l()?;
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[JValue::Object(&action), JValue::Object(&uri)],
        )?;
        env.call_method(activity, "startActivity", "(Landroid/content/Intent;)V", &[JValue::Object(&intent)])?;
        Ok(())
    });
}

/// Asks the Kotlin side to run the notification check now
/// (`dev.meteo.app.MeteoApp.checkNow`). App classes aren't visible to
/// `FindClass` from a native thread, so the activity's class loader loads it.
pub fn check_notifications_now(app: &AndroidApp) {
    let _ = with_activity(app, |env, activity| {
        let loader = env.call_method(activity, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])?.l()?;
        let name = env.new_string("dev.meteo.app.MeteoApp")?;
        let class = env
            .call_method(&loader, "loadClass", "(Ljava/lang/String;)Ljava/lang/Class;", &[JValue::Object(&name)])?
            .l()?;
        let class = jni::objects::JClass::from(class);
        env.call_static_method(&class, "checkNow", "(Landroid/content/Context;)V", &[JValue::Object(activity)])?;
        Ok(())
    });
}
