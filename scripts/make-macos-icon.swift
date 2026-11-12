// Draws the Météo app icon (sun behind a cloud on the clear-day sky) as a
// 1024×1024 PNG in macOS's rounded-square style. Same shapes as the Android
// adaptive icon (android/app/src/main/res/drawable/ic_launcher_*.xml), which
// uses a 108×108 grid.
//
//   scripts/make-macos-icon.sh   (renders this, then builds app/icons/Meteo.icns)
import AppKit

let size = 1024.0
let inset = 100.0                   // macOS icons leave a margin around the shape
let side = size - 2 * inset
// Android shows the central 72 units of its 108-grid (18…90); map that to the square.
let unit = side / 72.0
let origin = 18.0

let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocusFlipped(true)        // y down, like the Android vectors
let ctx = NSGraphicsContext.current!.cgContext

func p(_ x: Double, _ y: Double) -> CGPoint { CGPoint(x: inset + (x - origin) * unit, y: inset + (y - origin) * unit) }
func circle(_ cx: Double, _ cy: Double, _ r: Double) -> CGRect {
    CGRect(origin: p(cx - r, cy - r), size: CGSize(width: 2 * r * unit, height: 2 * r * unit))
}

// Rounded square with a soft shadow, filled with the sky gradient.
let square = CGPath(roundedRect: CGRect(x: inset, y: inset, width: side, height: side),
                    cornerWidth: side * 0.225, cornerHeight: side * 0.225, transform: nil)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: 10), blur: 28, color: NSColor(white: 0, alpha: 0.3).cgColor)
ctx.addPath(square)
ctx.setFillColor(NSColor.white.cgColor)
ctx.fillPath()
ctx.restoreGState()

ctx.saveGState()
ctx.addPath(square)
ctx.clip()
let sky = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                     colors: [NSColor(srgbRed: 0x2F/255, green: 0x80/255, blue: 0xED/255, alpha: 1).cgColor,
                              NSColor(srgbRed: 0x8E/255, green: 0xC5/255, blue: 0xFC/255, alpha: 1).cgColor] as CFArray,
                     locations: [0, 1])!
ctx.drawLinearGradient(sky, start: p(54, 0), end: p(54, 108), options: [])

// Sun.
ctx.setFillColor(NSColor(srgbRed: 1, green: 0xD5/255, blue: 0x4F/255, alpha: 1).cgColor)
ctx.fillEllipse(in: circle(64, 44, 14))

// Cloud: three puffs and a rounded base, with a faint shadow underneath.
ctx.setShadow(offset: CGSize(width: 0, height: 6), blur: 18, color: NSColor(white: 0, alpha: 0.18).cgColor)
ctx.beginTransparencyLayer(auxiliaryInfo: nil)
ctx.setFillColor(NSColor.white.cgColor)
ctx.fillEllipse(in: circle(42, 58, 11))
ctx.fillEllipse(in: circle(56, 52, 14))
ctx.fillEllipse(in: circle(68, 60, 9))
ctx.addPath(CGPath(roundedRect: CGRect(origin: p(30, 58), size: CGSize(width: 48 * unit, height: 12 * unit)),
                   cornerWidth: 6 * unit, cornerHeight: 6 * unit, transform: nil))
ctx.fillPath()
ctx.endTransparencyLayer()
ctx.restoreGState()
image.unlockFocus()

let rep = NSBitmapImageRep(data: image.tiffRepresentation!)!
let png = rep.representation(using: .png, properties: [:])!
try! png.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
