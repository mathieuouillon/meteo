//! The device's position on macOS, through CoreLocation (via objc2).
//!
//! Runs on the UI (main) thread: CLLocationManager delivers updates on the
//! run loop of the thread that created it, which is the app's main run loop
//! here. Like the Android side, it polls (with a Slint timer) instead of
//! implementing a delegate class.
//!
//! macOS only shows the permission prompt to a bundled app whose Info.plist
//! has NSLocationWhenInUseUsageDescription; see scripts/bundle-macos.sh.

#![allow(deprecated)] // CLGeocoder: Apple now points to MapKit, but it still works.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use block2::RcBlock;
use meteo_core::Location;
use objc2::rc::Retained;
use objc2_core_location::{
    CLAuthorizationStatus, CLGeocoder, CLLocation, CLLocationManager, CLPlacemark,
    kCLLocationAccuracyKilometer,
};
use objc2_foundation::{NSArray, NSError};

/// Time allowed for the user to answer the permission prompt.
const PERMISSION_TIMEOUT: Duration = Duration::from_secs(60);
/// Time allowed to get a position once permitted.
const FIX_TIMEOUT: Duration = Duration::from_secs(20);

type Done = Box<dyn FnOnce(Result<Location, String>)>;

struct Pending {
    manager: Retained<CLLocationManager>,
    started: Instant,
    granted_at: Option<Instant>,
    done: Done,
}

thread_local! {
    static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
    static POLL: slint::Timer = slint::Timer::default();
    /// Kept alive until its completion handler runs.
    static GEOCODER: RefCell<Option<Retained<CLGeocoder>>> = const { RefCell::new(None) };
}

/// Location works only from a bundled app (one with a bundle identifier
/// and the usage description); `cargo run` can't get permission.
pub fn available() -> bool {
    let bundle = objc2_foundation::NSBundle::mainBundle();
    bundle.bundleIdentifier().is_some()
        && bundle
            .objectForInfoDictionaryKey(&objc2_foundation::NSString::from_str("NSLocationWhenInUseUsageDescription"))
            .is_some()
}

/// Finds the device's position and calls `done` on the UI thread.
/// `quiet`: never show the permission prompt (startup refresh).
/// Must be called on the UI thread.
pub fn locate(quiet: bool, done: impl FnOnce(Result<Location, String>) + 'static) {
    let manager = unsafe { CLLocationManager::new() };
    let status = unsafe { manager.authorizationStatus() };
    if is_refused(status) || (quiet && status == CLAuthorizationStatus::NotDetermined) {
        done(Err(refused_message()));
        return;
    }
    unsafe {
        manager.setDesiredAccuracy(kCLLocationAccuracyKilometer);
        if status == CLAuthorizationStatus::NotDetermined {
            manager.requestWhenInUseAuthorization();
        }
        manager.startUpdatingLocation();
    }
    // A newer request replaces an older one (its callback is dropped).
    PENDING.with(|p| {
        if let Some(old) = p.borrow_mut().replace(Pending {
            manager,
            started: Instant::now(),
            granted_at: None,
            done: Box::new(done),
        }) {
            unsafe { old.manager.stopUpdatingLocation() };
        }
    });
    POLL.with(|t| t.start(slint::TimerMode::Repeated, Duration::from_millis(300), poll));
}

fn is_refused(status: CLAuthorizationStatus) -> bool {
    status == CLAuthorizationStatus::Denied || status == CLAuthorizationStatus::Restricted
}

fn refused_message() -> String {
    "Location is off for Météo. Turn it on in System Settings › Privacy & Security › Location Services.".into()
}

/// Checks the pending request; finishes it on success, refusal or timeout.
fn poll() {
    let outcome = PENDING.with(|p| {
        let mut slot = p.borrow_mut();
        let pending = slot.as_mut()?;
        let status = unsafe { pending.manager.authorizationStatus() };
        let result = if is_refused(status) {
            Some(Err(refused_message()))
        } else if status == CLAuthorizationStatus::NotDetermined {
            (pending.started.elapsed() > PERMISSION_TIMEOUT)
                .then(|| Err("Location permission wasn't granted".to_string()))
        } else {
            let granted_at = *pending.granted_at.get_or_insert_with(Instant::now);
            match unsafe { pending.manager.location() } {
                Some(location) => Some(Ok(location)),
                None if granted_at.elapsed() > FIX_TIMEOUT => {
                    Some(Err("Couldn't find your location. Is Wi-Fi on?".to_string()))
                }
                None => None,
            }
        }?;
        let pending = slot.take()?;
        unsafe { pending.manager.stopUpdatingLocation() };
        Some((result, pending.done))
    });
    let Some((result, done)) = outcome else { return };
    POLL.with(|t| t.stop());
    match result {
        Ok(location) => name_and_finish(location, done),
        Err(message) => done(Err(message)),
    }
}

/// Names the position with CLGeocoder, then calls `done`.
fn name_and_finish(location: Retained<CLLocation>, done: Done) {
    let c = unsafe { location.coordinate() };
    let (lat, lon) = (c.latitude, c.longitude);
    let done = RefCell::new(Some(done));
    let handler = RcBlock::new(move |placemarks: *mut NSArray<CLPlacemark>, _error: *mut NSError| {
        // SAFETY: CoreLocation passes a valid array or null.
        let first = unsafe { placemarks.as_ref() }.and_then(|a| a.firstObject());
        let text = |s: Option<Retained<objc2_foundation::NSString>>| s.map(|s| s.to_string());
        let (name, detail) = match first {
            Some(p) => unsafe {
                let detail = [text(p.administrativeArea()), text(p.country())]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", ");
                (text(p.locality()).unwrap_or_else(|| "My location".into()), detail)
            },
            None => ("My location".into(), String::new()),
        };
        if let Some(done) = done.borrow_mut().take() {
            done(Ok(Location::new(name, detail, lat, lon)));
        }
        GEOCODER.with(|g| g.borrow_mut().take());
    });
    let geocoder = unsafe { CLGeocoder::new() };
    unsafe { geocoder.reverseGeocodeLocation_completionHandler(&location, RcBlock::as_ptr(&handler)) };
    GEOCODER.with(|g| *g.borrow_mut() = Some(geocoder));
}
