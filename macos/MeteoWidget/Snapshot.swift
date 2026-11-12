// Renders the widget's views to PNGs with real data, to check the layout
// without adding the widget to the desktop:
//   scripts/bundle-macos.sh --snapshot <dir>
import AppKit
import SwiftUI

@main
struct Snapshot {
    @MainActor
    static func main() {
        let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "."
        let view = Core.state(at: .now).view
        for (name, size, wide) in [("small", CGSize(width: 170, height: 170), false),
                                   ("medium", CGSize(width: 364, height: 170), true)] {
            let content = MeteoWidgetContent(view: view, wide: wide)
                .padding(16)
                .frame(width: size.width, height: size.height)
                .background(SkyBackground(view: view))
                .clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
            let renderer = ImageRenderer(content: content)
            renderer.scale = 2
            guard let image = renderer.nsImage, let tiff = image.tiffRepresentation,
                  let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) else { continue }
            try? png.write(to: URL(fileURLWithPath: "\(out)/widget-\(name).png"))
            print("wrote \(out)/widget-\(name).png")
        }
    }
}
