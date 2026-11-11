#if canImport(AppKit)
import SwiftUI

/// Every Windows app the guest listed, opened from 가상 머신 > Windows 앱 > 모든 앱…: a search field and the list;
/// choosing an app opens it in Windows and closes the sheet.
@MainActor
struct HvfGuestStartAppsPanel: View {
    @ObservedObject var apps: HvfGuestStartAppsController
    let open: @MainActor (HvfGuestStartApp) -> Void
    @State private var search = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let shown = search.isEmpty ? apps.apps : apps.apps.filter { $0.name.localizedCaseInsensitiveContains(search) }
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Windows 앱").font(.headline)
                Spacer()
                Button("새로 고침") { apps.refresh() }.disabled(apps.status == .loading || apps.unavailableReason != nil)
                Button("닫기") { dismiss() }
            }
            TextField("검색", text: $search).textFieldStyle(.roundedBorder)
            if let reason = apps.unavailableReason ?? (apps.apps.isEmpty ? apps.statusText : nil) {
                Text(reason).foregroundStyle(.secondary).accessibilityIdentifier("runtime.windowsApps.status")
            }
            List(shown) { app in
                Button(app.name) { open(app); dismiss() }.buttonStyle(.plain)
            }
        }
        .padding().frame(minWidth: 380, minHeight: 420)
        .onAppear { if apps.unavailableReason == nil { apps.loadIfStale() } }
    }
}
#endif
