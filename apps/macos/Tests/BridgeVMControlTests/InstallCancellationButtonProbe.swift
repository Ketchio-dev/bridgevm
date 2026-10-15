import AppKit
import SwiftUI

/// Resolve the actual button environment and action through public SwiftUI
/// styling. This does not synthesize a physical click or exercise hit testing.
@MainActor
final class InstallCancellationButtonProbe {
    struct Entry {
        let enabled: Bool
        let trigger: () -> Void
        func activateIfEnabled() { if enabled { trigger() } }
    }
    var entries: [ObjectIdentifier: Entry] = [:]
}

@MainActor
struct InstallCancellationProbeStyle: PrimitiveButtonStyle {
    let probe: InstallCancellationButtonProbe
    func makeBody(configuration: Configuration) -> some View {
        VStack {
            configuration.label
            InstallCancellationProbeEntry(probe: probe, trigger: { configuration.trigger() })
        }
    }
}

private final class InstallCancellationProbeNode: NSView {
    weak var probe: InstallCancellationButtonProbe?
}

private struct InstallCancellationProbeEntry: NSViewRepresentable {
    let probe: InstallCancellationButtonProbe
    let trigger: () -> Void
    func makeNSView(context: Context) -> InstallCancellationProbeNode { InstallCancellationProbeNode() }
    func updateNSView(_ view: InstallCancellationProbeNode, context: Context) {
        view.probe = probe
        probe.entries[ObjectIdentifier(view)] = .init(enabled: context.environment.isEnabled, trigger: trigger)
    }
    static func dismantleNSView(_ view: InstallCancellationProbeNode, coordinator: ()) {
        view.probe?.entries.removeValue(forKey: ObjectIdentifier(view))
    }
}
