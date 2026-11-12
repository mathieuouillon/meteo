# Météo

A weather app for Android and macOS, written in Rust with a [Slint](https://slint.dev) UI.
Forecasts come from [Open-Meteo](https://open-meteo.com) (free, no API key).

```
core/        meteo-core: Open-Meteo client, models, weather codes → icons/colours,
             chart geometry, cache, saved cities, and the view models for the
             main screen and the widgets. No UI code.
app/         meteo: the Slint app (ui/main.slint) + glue in src/lib.rs.
widget-ffi/  Functions for the widgets (no UI code): JNI for Android (libmeteo_widget.so),
             C for macOS (libmeteo_widget.a).
macos/       The macOS WidgetKit extension (Swift).
android/     Gradle project: packages the Rust libraries + the Kotlin Glance widget.
scripts/     bundle-macos.sh builds Météo.app.
docs/        screenshots.
```

The UI only binds ready-to-display values from `MainScreen`; formatting and layout
maths stay in `core` so a future home-screen widget (Kotlin / Swift) can reuse them.

## Run on macOS

```bash
cargo run -p meteo
```

### As a macOS app (needed for location)

```bash
scripts/bundle-macos.sh          # release build → target/Météo.app
scripts/bundle-macos.sh --debug  # faster to compile
open target/Météo.app
```

The script wraps the binary in an `.app` with an `Info.plist` (bundle id
`dev.meteo.app`, the location usage text) and signs it ad hoc. macOS only asks for
location permission for a bundled app, so "Use my location" is hidden under
`cargo run`.

## Run on Android

One-time setup: the Android SDK with NDK `30.0.16248370`, JDK 17 or newer, the Rust
target, and `cargo-ndk`.

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
```

Build and install (emulator or device connected via `adb`):

```bash
cd android
./gradlew installDebug
```

Gradle's `cargoBuild` task runs `cargo ndk` first, building `libmeteo.so` (the Slint app,
started by `android.app.NativeActivity`) and `libmeteo_widget.so` (the widget's bridge).
`./gradlew assembleRelease` builds optimised Rust.

## Android widget

A home-screen widget showing the selected city: icon, temperature, condition, high/low,
and in its wide size the next 5 hours. Tapping it opens the app.

- The layout is Kotlin Glance ([MeteoWidget.kt](android/app/src/main/java/dev/meteo/app/widget/MeteoWidget.kt)).
  Its content comes from Rust: `RustBridge.state()` returns `meteo_core::WidgetState` as
  JSON, built from `places.json` and the city's cached forecast and moved forward to the
  current hour.
- If the cache is missing or older than 30 minutes, the widget calls `RustBridge.refresh()`,
  which downloads the forecast in Rust while the app is closed. Android also wakes the
  widget every 30 minutes.
- After every download, the app tells the widget to redraw (`notify_widgets`, a JNI
  broadcast), so switching city in the app updates the widget straight away.

## Tests

```bash
cargo test -p meteo-core                    # offline tests
cargo test -p meteo-core -- --include-ignored  # also hits the live API
```

## Rain amounts

- The hourly chart shows the expected amount under each hour ("1.2 mm"), below the
  chance of rain.
- Each day in the 7-day list shows its total. Tap a day to open its rain hour by hour:
  a bar chart for 00h–23h that scrolls sideways, with the amount above each wet hour.
  Bars share one scale for the whole week (at least 2 mm), so a wet day looks wetter
  than a drizzly one. Today opens at the current hour, other days at their first wet
  hour.
- Amounts are Open-Meteo's `precipitation`: rain, showers and snow as water, in mm.

## Data source

Tap "Data: … ›" at the bottom of the screen to pick the weather model.
[core/src/source.rs](core/src/source.rs) lists them: Best match (Open-Meteo's default
blend), Météo-France (AROME/ARPEGE), ECMWF, DWD ICON, NOAA GFS, UK Met Office,
MET Norway, KNMI, DMI, Environment Canada and JMA. They all come through Open-Meteo,
so there are still no API keys.

- Regional models are more detailed near home but may cover fewer days (Météo-France:
  about 4) or have no rain probability (Météo-France, JMA). Parsing stops at the first
  missing hour or day instead of failing.
- The choice is saved in `settings.json`, and the widget uses it too. Each source has
  its own cache file (`forecast-<lat>_<lon>-<model>.json`), so switching back and
  forth works offline.

## Animated weather

Behind the forecast, the sky moves with the current weather: rain, snow, drifting
clouds, fog, twinkling stars at night, and lightning in storms.

- [core/src/scene.rs](core/src/scene.rs) is a small particle simulation. Raindrops
  fall under gravity with air drag up to a terminal speed that depends on their size,
  and are pushed sideways by the forecast's wind, direction included. Snowflakes
  sway as they fall. Clouds drift with parallax (bigger ones are closer and move
  faster). Lightning is a flash, a quick second flicker and a random pause.
- Each particle layer reaches Slint as one path string per frame, so hundreds of
  drops are drawn with a single element. The rest is
  [app/src/animation.rs](app/src/animation.rs) (a timer) and the `Sky` component in
  `main.slint`.
- It runs at 60 fps when something falls and 30 fps for clouds, fog and stars. It
  pauses behind the city picker and when the app is in the background.

To see a given weather whatever the forecast says:

```bash
METEO_WEATHER=storm cargo run -p meteo   # clear, night, partly, cloudy, fog, drizzle, rain, snow, storm
```

## macOS widget

A WidgetKit widget in small and medium sizes, the same content as the Android
widget. The medium size adds the next 5 hours. Clicking it opens the app.

```bash
scripts/bundle-macos.sh --install   # builds Météo.app, copies it to ~/Applications
open ~/Applications/Météo.app       # run it once so macOS lists the widget
```

Then right-click the desktop, choose **Edit Widgets…**, search for "Météo" and drag it
out. It also works in Notification Center.

**The widget needs a developer certificate.** macOS refuses to start a widget extension
signed ad hoc (chronod logs `RBSServiceErrorDomain Code=1 "Client not authorized"`), so
it never shows in the gallery. A free Apple ID is enough: Xcode › Settings › Accounts ›
add your Apple ID › Manage Certificates › **+** › Apple Development. The bundle script
then signs with that certificate automatically (or set `MACOS_SIGN_IDENTITY`). Without
one the app still works, only the widget doesn't.

- [macos/MeteoWidget](macos/MeteoWidget) holds the Swift code. It only does layout: its
  content comes from the Rust core through a small C API (`meteo_widget.h`, built from
  `widget-ffi` as a static library and linked into the extension).
- Timeline: one entry now and one at the start of each of the next 6 hours, so "Now"
  moves on without waking the widget. It reloads every 30 minutes and downloads a new
  forecast itself when the cache is older than that.
- Data: widget extensions must be sandboxed. Without a development team there is no
  App Group to share a container, so the extension's entitlements allow just the app's
  data folder (`~/Library/Application Support/dev.meteo.app/`, read and write).
  Signing is described above.
- To check the layout without placing the widget,
  `scripts/bundle-macos.sh --snapshot <dir>` renders both sizes with real data to PNG.

## Accent colour

Buttons and selections use the system accent; the sky colours stay tied to the
weather. On Android 12+ that's the Material You colour taken from the wallpaper
(`system_accent1_200` / `_500`, read through JNI). On macOS it's the accent colour
chosen in System Settings, lightened for the dark sky. Elsewhere, and on older
Android, the app's own blue is used. See [app/src/accent.rs](app/src/accent.rs).
The colours are read when the app starts.

## App icon

A sun behind a cloud on the clear-day sky.

- Android: an adaptive icon made of vector drawables
  (`android/app/src/main/res/drawable/ic_launcher_*.xml`), with a monochrome layer so
  Android 13+ can tint it with the wallpaper colours (themed icons).
- macOS: `app/icons/Meteo.icns`, drawn from the same shapes by
  `scripts/make-macos-icon.sh`. `scripts/bundle-macos.sh` adds it to `Météo.app`.

## Cities and location

Tap the city name to open the city picker:

- **Search** uses Open-Meteo's geocoding API (`meteo_core::search_cities`). Tap a
  result to save and show it.
- **Saved cities** (`meteo_core::Places`) are stored in `places.json` next to the
  cache. Tap one to switch, ✕ to remove it. The list is never empty (Paris by default).
- **Use my location** asks for location permission, finds the device's position,
  and names it (Android's `Geocoder`, macOS's `CLGeocoder`). It becomes a "📍" entry at
  the top of the list, refreshed each time the app opens. This is all JNI from Rust
  ([app/src/location_android.rs](app/src/location_android.rs)). A NativeActivity gets
  no Java callbacks, so the code polls the permission state, and it starts location
  updates sent to a `PendingIntent` (so the provider produces a fix) while it polls
  `getLastKnownLocation`.

On macOS, [app/src/location_macos.rs](app/src/location_macos.rs) uses CoreLocation via
`objc2`, on the main thread, polling `CLLocationManager` with a Slint timer instead of
a delegate class. It only works from `Météo.app` (see above).

## Offline cache

After each successful download, `core` saves the forecast as JSON, one file per city
(`meteo_core::cache`). On startup the app shows the cached forecast at once, then
refreshes it. A cached forecast is moved forward to the current time first: past
hours and days are dropped and the first ones relabelled "Now" and "Today" (48 hours
are kept so there is still a full day to show). If a refresh fails, the cached
forecast stays on screen with "Offline · updated 3 h ago".

| Platform | Cache file |
|----------|------------|
| macOS    | `~/Library/Application Support/dev.meteo.app/forecast-<lat>_<lon>.json` |
| Android  | the app's internal files directory (`files/forecast-<lat>_<lon>.json`) |

The home-screen widgets will read the same file. On macOS a WidgetKit extension
can't read another app's Application Support folder, so the cache will move to a
shared App Group container then.

## Notes

- **Renderer:** the app uses Slint's Skia renderer. The default FemtoVG renderer
  drops large colour emoji.
- **Slint `Path`:** chart paths set `fit: fill`. The default `contain` rescales
  and re-centres a stroked path, which misaligns the curve with the hour dots.
- **TLS on Android:** `core` uses bundled Mozilla root certificates on Android
  (`webpki-root-certs`). reqwest's default platform verifier needs JNI
  initialisation that a NativeActivity app doesn't do.
