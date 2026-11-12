// WidgetKit entry point: when to update, and which sizes exist.

import SwiftUI
import WidgetKit

struct Entry: TimelineEntry {
    let date: Date
    let view: WidgetView?
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> Entry {
        Entry(date: .now, view: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (Entry) -> Void) {
        completion(Entry(date: .now, view: Core.state(at: .now).view))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<Entry>) -> Void) {
        DispatchQueue.global(qos: .utility).async {
            let now = Date.now
            // Download first if the cache is old or missing (the app may be closed).
            if Core.state(at: now).needsRefresh {
                _ = Core.refresh()
            }
            // One entry now and one at the start of each of the next hours, so
            // "Now" and the hour labels move on without waking the widget.
            var dates = [now]
            if let nextHour = Calendar.current.nextDate(after: now, matching: DateComponents(minute: 0), matchingPolicy: .nextTime) {
                dates += (0..<6).map { nextHour.addingTimeInterval(Double($0) * 3600) }
            }
            let entries = dates.map { Entry(date: $0, view: Core.state(at: $0).view) }
            completion(Timeline(entries: entries, policy: .after(now.addingTimeInterval(30 * 60))))
        }
    }
}

struct MeteoWidgetEntryView: View {
    @Environment(\.widgetFamily) private var family
    let entry: Entry

    var body: some View {
        MeteoWidgetContent(view: entry.view, wide: family != .systemSmall)
            .containerBackground(for: .widget) { SkyBackground(view: entry.view) }
    }
}

@main
struct MeteoWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "dev.meteo.app.widget", provider: Provider()) { entry in
            MeteoWidgetEntryView(entry: entry)
        }
        .configurationDisplayName("Météo")
        .description("Current weather and the next hours for the city selected in Météo.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}
