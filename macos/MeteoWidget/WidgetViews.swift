// Data and layout of the Météo widget. Everything shown comes from Rust
// (meteo_core::WidgetState via meteo_widget.h); this file only lays it out.

import Foundation
import SwiftUI

struct WidgetHour: Decodable, Hashable {
    let label: String
    let icon: String
    let temperature: String
}

struct WidgetView: Decodable {
    let city: String
    let isDevice: Bool
    let icon: String
    let temperature: String
    let condition: String
    let highLow: String
    let updated: String
    let skyTop: UInt32
    let skyBottom: UInt32
    let hours: [WidgetHour]
}

struct WidgetState: Decodable {
    let needsRefresh: Bool
    let view: WidgetView?
}

/// Calls into the Rust core.
enum Core {
    /// The app's data folder. The widget is sandboxed, so NSHomeDirectory()
    /// points into its container; the real home comes from the user database.
    /// Its entitlements allow this one folder.
    static let dataDir: String = {
        let home = getpwuid(getuid()).flatMap { String(cString: $0.pointee.pw_dir) } ?? NSHomeDirectory()
        return home + "/Library/Application Support/dev.meteo.app"
    }()

    /// The widget's content as it should look at `date`.
    static func state(at date: Date) -> WidgetState {
        let empty = WidgetState(needsRefresh: true, view: nil)
        guard let raw = meteo_widget_state(dataDir, Int64(date.timeIntervalSince1970)) else { return empty }
        defer { meteo_string_free(raw) }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return (try? decoder.decode(WidgetState.self, from: Data(String(cString: raw).utf8))) ?? empty
    }

    /// Downloads the selected city's forecast (blocking).
    static func refresh() -> Bool {
        meteo_widget_refresh(dataDir)
    }
}

extension Color {
    init(rgb: UInt32) {
        self.init(.sRGB,
                  red: Double((rgb >> 16) & 0xFF) / 255,
                  green: Double((rgb >> 8) & 0xFF) / 255,
                  blue: Double(rgb & 0xFF) / 255)
    }
}

/// The sky gradient behind the widget, from the current weather.
struct SkyBackground: View {
    let view: WidgetView?
    var body: some View {
        LinearGradient(colors: [Color(rgb: view?.skyTop ?? 0x2F80ED), Color(rgb: view?.skyBottom ?? 0x8EC5FC)],
                       startPoint: .top, endPoint: .bottom)
    }
}

/// Small: the current weather. Medium: plus the next hours.
struct MeteoWidgetContent: View {
    let view: WidgetView?
    let wide: Bool

    var body: some View {
        if let v = view {
            Group {
                if wide {
                    // Medium: the city across the top, so it isn't squeezed by the hours.
                    VStack(alignment: .leading, spacing: 4) {
                        city(v)
                        HStack(alignment: .center, spacing: 8) {
                            conditions(v)
                            Spacer(minLength: 0)
                            hours(v)
                        }
                        Spacer(minLength: 0)
                        updated(v)
                    }
                } else {
                    VStack(alignment: .leading, spacing: 2) {
                        city(v)
                        conditions(v)
                        Spacer(minLength: 0)
                        updated(v)
                    }
                }
            }
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else {
            VStack(alignment: .leading, spacing: 4) {
                Text("Météo").font(.headline)
                Text("Open the app to choose a city").font(.caption).opacity(0.8)
            }
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
    }

    private func city(_ v: WidgetView) -> some View {
        Text((v.isDevice ? "📍 " : "") + v.city)
            .font(.system(size: 14, weight: .semibold))
            .lineLimit(1)
    }

    private func conditions(_ v: WidgetView) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 4) {
                Text(v.icon).font(.system(size: 24))
                Text(v.temperature).font(.system(size: 38, weight: .light))
            }
            Text(v.condition).font(.system(size: 12)).lineLimit(1)
            Text(v.highLow).font(.system(size: 11)).opacity(0.8).lineLimit(1)
        }
    }

    private func updated(_ v: WidgetView) -> some View {
        Text(v.updated).font(.system(size: 9)).opacity(0.6).lineLimit(1)
    }

    private func hours(_ v: WidgetView) -> some View {
        HStack(spacing: 0) {
            ForEach(v.hours, id: \.self) { h in
                VStack(spacing: 6) {
                    Text(h.label).font(.system(size: 11)).opacity(0.8)
                    Text(h.icon).font(.system(size: 18))
                    Text(h.temperature).font(.system(size: 13, weight: .medium))
                }
                .frame(width: 40)
            }
        }
    }
}
