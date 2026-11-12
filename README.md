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

## Notes

- **Renderer:** the app uses Slint's Skia renderer. The default FemtoVG renderer
  drops large colour emoji.
- **Slint `Path`:** chart paths set `fit: fill`. The default `contain` rescales
  and re-centres a stroked path, which misaligns the curve with the hour dots.
- **TLS on Android:** `core` uses bundled Mozilla root certificates on Android
  (`webpki-root-certs`). reqwest's default platform verifier needs JNI
  initialisation that a NativeActivity app doesn't do.
