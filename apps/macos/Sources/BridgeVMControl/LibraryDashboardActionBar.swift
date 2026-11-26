import SwiftUI

struct LibraryDashboardActionBar: View {
    let runtimeActive: Bool
    let lifecycleBusy: Bool
    let open: () -> Void
    let showAdvanced: () -> Void
    var showSettings: (() -> Void)?
    let refresh: () -> Void
    var clone: (() -> Void)?
    var cloneDisabled = false

    var body: some View {
        HStack(spacing: 8) {
            Button(lifecycleBusy ? "작업 중" : (runtimeActive ? "창 열기" : "시작"),
                   systemImage: runtimeActive ? "macwindow" : "play.fill", action: open)
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .fontWeight(.semibold)
                .disabled(lifecycleBusy)
            Spacer(minLength: 4)
            if let showSettings {
                Button("Settings", systemImage: "slider.horizontal.3", action: showSettings)
                    .accessibilityIdentifier("bridgevm.runtime.settings.open")
            }
            Button("고급", systemImage: "gearshape", action: showAdvanced)
                .accessibilityIdentifier("bridgevm.dashboard.advanced")
            Menu {
                Button("새로고침", systemImage: "arrow.clockwise", action: refresh)
                if let clone {
                    Button("복제", systemImage: "plus.square.on.square", action: clone)
                        .disabled(cloneDisabled)
                }
            } label: {
                Image(systemName: "ellipsis").frame(width: 26, height: 26)
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            .help("추가 작업").accessibilityLabel("추가 작업")
            .accessibilityIdentifier("bridgevm.dashboard.actions")
        }
        .labelStyle(.titleAndIcon)
    }
}
