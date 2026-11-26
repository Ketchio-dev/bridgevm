import Foundation
import SwiftUI

/// Every runtime poll (100 ms, or 10 ms while ordered input is pumped) recomputes the guest
/// heartbeat age. It is published here, not by `HvfEngineSession`, so those ticks re-render only
/// the two heartbeat texts below instead of every session observer (the live display included).
@MainActor
final class HvfRuntimeHeartbeat: ObservableObject {
    @Published var age: TimeInterval?
}

extension HvfEngineSession {
    var lastHeartbeatAge: TimeInterval? {
        get { heartbeat.age }
        set { heartbeat.age = newValue }
    }
}

/// The runtime status card's heartbeat item: same layout, identifier and value as the other items.
struct HvfRuntimeHeartbeatItem: View {
    @ObservedObject var heartbeat: HvfRuntimeHeartbeat

    var body: some View {
        let value = heartbeat.age.map { String(format: "%.1f s ago", $0) } ?? "-"
        VStack(alignment: .leading, spacing: 2) {
            Text("Guest agent response").font(.caption).foregroundColor(.secondary)
            Text(value).font(.body.monospaced()).accessibilityIdentifier("bridgevm.windows.runtime.heartbeat")
                .accessibilityValue(value)
        }
    }
}

/// The retained runtime's last-response caption, absent until a heartbeat has been seen.
struct RetainedRuntimeHeartbeatText: View {
    @ObservedObject var heartbeat: HvfRuntimeHeartbeat

    var body: some View {
        if let age = heartbeat.age {
            Text("Last response \(age, specifier: "%.1f") s ago").font(.caption)
        }
    }
}
