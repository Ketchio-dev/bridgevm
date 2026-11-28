import SwiftUI

/// Shown on each running VM while running VMs together use more memory than this Mac can spare
/// (ControlAppPower.sharedMemoryNotice); it goes away when one of them stops.
struct HvfSharedMemoryNoticeView: View {
    @ObservedObject private var power: ControlAppPower
    private let runtime: UUID

    init(session: HvfEngineSession) {
        _power = ObservedObject(wrappedValue: session.sleepPrevention)
        runtime = session.sleepPreventionID
    }

    var body: some View {
        if let text = power.sharedMemoryNotice(for: runtime) {
            Label(text, systemImage: "memorychip").font(.caption).foregroundStyle(.secondary)
                .accessibilityIdentifier("bridgevm.runtime.memory.notice")
        }
    }
}
