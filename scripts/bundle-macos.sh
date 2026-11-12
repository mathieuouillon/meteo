#!/bin/sh
# Builds Météo.app for macOS: the Slint binary plus an Info.plist (bundle id,
# location usage text), signed ad hoc so macOS's privacy system (TCC) can
# remember the location permission for it.
#
#   scripts/bundle-macos.sh            # release build
#   scripts/bundle-macos.sh --debug    # debug build (faster to compile)
#   open target/Météo.app
set -eu
cd "$(dirname "$0")/.."

profile=release
cargo_flags=--release
if [ "${1:-}" = "--debug" ]; then
    profile=debug
    cargo_flags=
fi

cargo build -p meteo $cargo_flags

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
app="target/Météo.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$profile/meteo" "$app/Contents/MacOS/meteo"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>          <string>dev.meteo.app</string>
    <key>CFBundleName</key>                <string>Météo</string>
    <key>CFBundleDisplayName</key>         <string>Météo</string>
    <key>CFBundleExecutable</key>          <string>meteo</string>
    <key>CFBundlePackageType</key>         <string>APPL</string>
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

codesign --force --sign - "$app"
echo "Built $app"
