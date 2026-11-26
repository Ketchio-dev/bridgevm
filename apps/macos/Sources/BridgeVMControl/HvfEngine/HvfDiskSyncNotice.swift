import Combine
import SwiftUI

/// A later successful sync cannot undo the earlier durability failure, so this stays until
/// dismissed or the next run; a guest reboot is not a new run.
@MainActor
final class HvfDiskSyncNotice: ObservableObject {
    static let text = "BridgeVM couldn’t finish saving the Windows disk. Check your Mac’s free space and drive connection, then shut down Windows and start it again."
    @Published private(set) var isVisible = false
    @Published private(set) var finalSaveFailed = false
    private var reported = false

    func consume(_ event: BvAgentEvent) {
        if case .diskFinalSaveFailed = event { finalSaveFailed = true; return }
        guard case .diskDurabilityFailed = event, !reported else { return }
        reported = true
        isVisible = true
    }
    func dismiss() { isVisible = false }
    func reset() { reported = false; isVisible = false; finalSaveFailed = false }
}

struct HvfDiskSyncNoticeView: View {
    @ObservedObject var notice: HvfDiskSyncNotice

    var body: some View {
        if notice.isVisible {
            HvfNoticeStrip(identifier: "bridgevm.storage.durability", dismiss: { notice.dismiss() }) {
                Text(HvfDiskSyncNotice.text)
            }
        }
    }
}
