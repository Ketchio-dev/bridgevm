#if canImport(AppKit)
import SwiftUI

/// The guest's window list, opened from the Virtual Machine menu's Show Open Windows… item.
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
                Text("Open Windows").font(.headline)
                Spacer()
                Button("Refresh") { model.refresh() }.disabled(model.isLoading)
                Button("Close") { dismiss() }
            }
            Text(model.status).accessibilityIdentifier("runtime.windows.status")
            List(model.records) { record in
                VStack(alignment: .leading) {
                    Text(record.title)
                    Text("\(record.width) × \(record.height)")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            Text("This list is read-only. Use the VM display to interact with these windows.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding().frame(minWidth: 440, minHeight: 320)
        .onAppear { model.start() }
        .onDisappear { model.stop() }
    }
}
#endif
