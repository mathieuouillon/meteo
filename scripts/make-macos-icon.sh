#!/bin/sh
# Rebuilds app/icons/Meteo.icns from scripts/make-macos-icon.swift.
set -eu
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)
swift scripts/make-macos-icon.swift "$tmp/icon-1024.png"
mkdir "$tmp/Meteo.iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size "$tmp/icon-1024.png" --out "$tmp/Meteo.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double "$tmp/icon-1024.png" --out "$tmp/Meteo.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
mkdir -p app/icons
iconutil -c icns "$tmp/Meteo.iconset" -o app/icons/Meteo.icns
rm -rf "$tmp"
echo "Wrote app/icons/Meteo.icns"
