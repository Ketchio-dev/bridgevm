#if canImport(AppKit)
import SwiftUI

/// The guest's window list, opened from the 가상 머신 menu's Windows 창 목록… item.
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
                Text("Windows 창 목록").font(.headline)
                Spacer()
                Button("새로 고침") { model.refresh() }.disabled(model.isLoading)
                Button("닫기") { dismiss() }
            }
            Text(model.status).accessibilityIdentifier("runtime.windows.status")
            List(model.records) { record in
                VStack(alignment: .leading) {
                    Text(record.title)
                    Text("PID \(record.processID) | \(record.width) x \(record.height) | (\(record.x), \(record.y))")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            Text("읽기 전용 목록입니다. Windows 창을 Mac 창으로 따로 여는 기능은 아직 없습니다.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .padding().frame(minWidth: 440, minHeight: 320)
        .onAppear { model.start() }
        .onDisappear { model.stop() }
    }
}
#endif
