import SwiftUI

/// Shown on each running VM and Windows install while BridgeVM can't keep the Mac awake
/// (ControlAppPower.keepAwakeNotice), instead of only in the log.
struct HvfKeepAwakeNoticeView: View {
    @ObservedObject var power: ControlAppPower
    let runtime: UUID

    var body: some View {
        if let text = power.keepAwakeNotice(for: runtime) {
            Label(text, systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange)
                .accessibilityIdentifier("bridgevm.runtime.keep-awake.notice")
        }
    }
}
