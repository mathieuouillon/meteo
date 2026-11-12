# Météo

A weather app for Android and macOS, written in Rust with a [Slint](https://slint.dev) UI.
Forecasts come from [Open-Meteo](https://open-meteo.com) (free, no API key).

```
core/   meteo-core: Open-Meteo client, models, weather codes → icons/colours,
        chart geometry, and the `MainScreen` view model. No UI code.
app/    meteo: the Slint app (ui/main.slint) + glue in src/lib.rs.
docs/   screenshots.
```

The UI only binds ready-to-display values from `MainScreen`; formatting and layout
maths stay in `core` so a future home-screen widget (Kotlin / Swift) can reuse them.

## Run on macOS

```bash
cargo run -p meteo
```

## Run on Android

One-time setup: Android SDK + NDK, the Rust target, and `cargo-apk`.

```bash
rustup target add aarch64-linux-android
cargo install cargo-apk
```

Build, install and launch (emulator or device connected via `adb`):

```bash
export ANDROID_HOME=~/Library/Android/sdk
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/30.0.16248370
cargo apk run -p meteo --lib
```

`cargo apk run` keeps streaming the app's logcat after launch; press Ctrl-C to stop
it (the app keeps running). Use `cargo apk build -p meteo --lib` to only build.

## Tests

```bash
cargo test -p meteo-core                    # offline tests
cargo test -p meteo-core -- --include-ignored  # also hits the live API
```

## Cities and location

Tap the city name to open the city picker:

- **Search** uses Open-Meteo's geocoding API (`meteo_core::search_cities`). Tap a
  result to save and show it.
- **Saved cities** (`meteo_core::Places`) are stored in `places.json` next to the
  cache. Tap one to switch, ✕ to remove it. The list is never empty (Paris by default).
- **Use my location** (Android only for now) asks for location permission, finds the
  device's position, and names it with Android's `Geocoder`. It becomes a "📍" entry at
  the top of the list, refreshed each time the app opens. This is all JNI from Rust
  ([app/src/location_android.rs](app/src/location_android.rs)). A NativeActivity gets
  no Java callbacks, so the code polls the permission state, and it starts location
  updates sent to a `PendingIntent` (so the provider produces a fix) while it polls
  `getLastKnownLocation`.

On macOS, location needs a proper `.app` bundle with a usage description
(`NSLocationWhenInUseUsageDescription`), so it isn't wired up yet.

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
