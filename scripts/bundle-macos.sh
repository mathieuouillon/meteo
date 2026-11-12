#!/bin/sh
# Builds Météo.app for macOS: the Slint binary plus an Info.plist (bundle id,
# location usage text), the WidgetKit widget extension (Swift, linked with the
# Rust widget-ffi library), and the icon. Everything is signed ad hoc, so
# macOS's privacy system can remember the location permission and the widget
# gets its sandbox entitlements.
#
#   scripts/bundle-macos.sh                     # release build
#   scripts/bundle-macos.sh --debug             # debug build (faster to compile)
#   scripts/bundle-macos.sh --snapshot <dir>    # also render the widget to PNGs
#   scripts/bundle-macos.sh --install           # also copy to ~/Applications and register
#   open target/Météo.app
#
# Signing: macOS only runs widget extensions signed with a developer
# certificate. A free Apple ID gives one (Xcode › Settings › Accounts ›
# Manage Certificates › + › Apple Development); it's picked up automatically.
# Without one the app is signed ad hoc and works, but the widget doesn't.
# MACOS_SIGN_IDENTITY overrides the choice.
set -eu
cd "$(dirname "$0")/.."

profile=release
cargo_flags=--release
snapshot_dir=
install=
while [ $# -gt 0 ]; do
    case "$1" in
        --debug) profile=debug; cargo_flags= ;;
        --snapshot) snapshot_dir="$2"; shift ;;
        --install) install=1 ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
    shift
done

# The widget runs on macOS 14+: build the C code in Rust dependencies (the
# TLS library) for it too. (MACOSX_DEPLOYMENT_TARGET would also do it, but it
# applies to proc-macro crates as well and breaks slint's.)
host=$(rustc -vV | sed -n 's/^host: //p')
export "CFLAGS_$(echo "$host" | tr - _)=-mmacosx-version-min=14.0"
cargo build -p meteo -p meteo-widget-ffi $cargo_flags

identity="${MACOS_SIGN_IDENTITY:-$(security find-identity -v -p codesigning | sed -n 's/.*) \([0-9A-F]\{40\}\) "Apple Development.*/\1/p' | head -n 1)}"
identity="${identity:--}"
if [ "$identity" = "-" ]; then
    echo "note: no Apple Development certificate found; signing ad hoc (the widget won't load)" >&2
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app="target/Météo.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/meteo" "$app/Contents/MacOS/meteo"
cp app/icons/Meteo.icns "$app/Contents/Resources/Meteo.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>          <string>dev.meteo.app</string>
    <key>CFBundleInfoDictionaryVersion</key> <string>6.0</string>
    <key>CFBundleSupportedPlatforms</key>  <array><string>MacOSX</string></array>
    <key>CFBundleName</key>                <string>Météo</string>
    <key>CFBundleDisplayName</key>         <string>Météo</string>
    <key>CFBundleExecutable</key>          <string>meteo</string>
    <key>CFBundlePackageType</key>         <string>APPL</string>
    <key>CFBundleIconFile</key>            <string>Meteo</string>
    <key>CFBundleShortVersionString</key>  <string>$version</string>
    <key>CFBundleVersion</key>             <string>$version</string>
    <key>LSMinimumSystemVersion</key>      <string>11.0</string>
    <key>LSApplicationCategoryType</key>   <string>public.app-category.weather</string>
    <key>NSHighResolutionCapable</key>     <true/>
    <key>NSLocationWhenInUseUsageDescription</key>
    <string>Météo uses your location to show the weather where you are.</string>
    <key>NSLocationUsageDescription</key>
    <string>Météo uses your location to show the weather where you are.</string>
</dict>
</plist>
PLIST

# ── Widget extension ──────────────────────────────────────────────────────
widget_src=macos/MeteoWidget
sdk=$(xcrun --sdk macosx --show-sdk-path)
arch=$(uname -m)
swift_flags="-O -parse-as-library -target $arch-apple-macos14.0 -sdk $sdk -import-objc-header $widget_src/meteo_widget.h"
# Libraries the Rust static library needs (cargo's native-static-libs).
rust_libs="target/$profile/libmeteo_widget.a -framework Security -framework SystemConfiguration -framework CoreFoundation -liconv"

appex="$app/Contents/PlugIns/MeteoWidget.appex"
mkdir -p "$appex/Contents/MacOS"
xcrun swiftc $swift_flags -application-extension \
    "$widget_src/MeteoWidget.swift" "$widget_src/WidgetViews.swift" $rust_libs \
    -o "$appex/Contents/MacOS/MeteoWidget"
sed "s/VERSION/$version/" "$widget_src/Info.plist" > "$appex/Contents/Info.plist"
codesign --force --sign "$identity" --entitlements "$widget_src/MeteoWidget.entitlements" "$appex"

codesign --force --sign "$identity" "$app"

if [ -n "$install" ]; then
    lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/Météo.app"
    cp -R "$app" "$HOME/Applications/"
    "$lsregister" -f "$HOME/Applications/Météo.app"
    echo "Installed $HOME/Applications/Météo.app"
fi

if [ -n "$snapshot_dir" ]; then
    mkdir -p "$snapshot_dir"
    xcrun swiftc $swift_flags "$widget_src/Snapshot.swift" "$widget_src/WidgetViews.swift" $rust_libs \
        -o target/meteo-widget-snapshot
    target/meteo-widget-snapshot "$snapshot_dir"
fi
echo "Built $app"
