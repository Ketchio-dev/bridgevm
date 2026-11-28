import SwiftUI

struct LibraryDashboardActionBar: View {
    let runtimeActive: Bool
    let lifecycleBusy: Bool
    let open: () -> Void
    let showAdvanced: () -> Void
    var showSettings: (() -> Void)?
    /// Checks the VM process and reads the library again, like Refresh Library in the command palette
    /// that messages point to.
    let refresh: () -> Void
    var clone: (() -> Void)?
    var cloneDisabled = false

    var body: some View {
        HStack(spacing: 8) {
            Button(lifecycleBusy ? "Working" : (runtimeActive ? "Open Display" : "Start"),
                   systemImage: runtimeActive ? "macwindow" : "play.fill", action: open)
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .fontWeight(.semibold)
                .disabled(lifecycleBusy)
                .accessibilityIdentifier("bridgevm.dashboard.primary")
            Spacer(minLength: 4)
            if let showSettings {
                Button("Settings", action: showSettings)
                    .accessibilityIdentifier("bridgevm.runtime.settings.open")
            }
            Button("Advanced", action: showAdvanced)
                .accessibilityIdentifier("bridgevm.dashboard.advanced")
            Menu {
                Button("Refresh Library", systemImage: "arrow.clockwise", action: refresh)
                if let clone {
                    Button("Duplicate", systemImage: "plus.square.on.square", action: clone)
                        .disabled(cloneDisabled)
                }
            } label: {
                Image(systemName: "ellipsis").frame(width: 26, height: 26)
            }
            .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
            .help("More Actions").accessibilityLabel("More Actions")
            .accessibilityIdentifier("bridgevm.dashboard.actions")
        }
        .labelStyle(.titleAndIcon)
    }
}
