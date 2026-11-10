#if canImport(AppKit)
import SwiftUI

/// The guest's window list, opened from the 가상 머신 menu's Guest windows… item.
@MainActor
struct HvfWindowInventoryPanel: View {
    @StateObject private var model: HvfWindowInventoryController
    @Environment(\.dismiss) private var dismiss
    init(session: HvfEngineSession) {
        _model = StateObject(wrappedValue: HvfWindowInventoryController(session: session))
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Guest windows").font(.headline)
                Spacer()
                Button("Refresh") { model.refresh() }.disabled(model.isLoading)
                Button("Close") { dismiss() }
            }
            Text(model.status).accessibilityIdentifier("runtime.windows.status")
            List(model.records) { record in
                VStack(alignment: .leading) {
                    Text(record.title)
                    Text("PID \(record.processID) | \(record.width) x \(record.height) | (\(record.x), \(record.y))")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            Text("Read-only inventory. Independent guest-window presentation is not connected.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding().frame(minWidth: 440, minHeight: 320)
        .onAppear { model.start() }
        .onDisappear { model.stop() }
    }
}
#endif
