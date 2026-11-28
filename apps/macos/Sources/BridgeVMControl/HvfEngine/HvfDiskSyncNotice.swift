import Combine
import SwiftUI

/// A later successful sync cannot undo the earlier durability failure, so this stays until
/// dismissed or the next run; a guest reboot is not a new run. The drive that cannot give
/// space back (`BVSTORAGE trim-unsupported`, nvme/dataset_management.rs) is told the same way.
@MainActor
final class HvfDiskSyncNotice: ObservableObject {
    static let text = "BridgeVM couldn’t finish saving the Windows disk. Check your Mac’s free space and drive connection, then shut down Windows and start it again."
    static let trimText = "The drive this VM is on can’t give back the space Windows frees, so the VM’s disk file won’t shrink. Move the VM to an APFS drive to save space."
    @Published private(set) var isVisible = false
    @Published private(set) var finalSaveFailed = false
    @Published private(set) var trimUnsupportedVisible = false
    private var reported = false
    private var trimReported = false

    func consume(_ event: BvAgentEvent) {
        if case .diskFinalSaveFailed = event { finalSaveFailed = true; return }
        if case .diskTrimUnsupported = event, !trimReported {
            trimReported = true
            trimUnsupportedVisible = true
            return
        }
        guard case .diskDurabilityFailed = event, !reported else { return }
        reported = true
        isVisible = true
    }
    func dismiss() { isVisible = false }
    func dismissTrimUnsupported() { trimUnsupportedVisible = false }
    func reset() {
        reported = false; isVisible = false; finalSaveFailed = false
        trimReported = false; trimUnsupportedVisible = false
    }
}

struct HvfDiskSyncNoticeView: View {
    @ObservedObject var notice: HvfDiskSyncNotice

    var body: some View {
        if notice.isVisible {
            HvfNoticeStrip(identifier: "bridgevm.storage.durability", dismiss: { notice.dismiss() }) {
                Text(HvfDiskSyncNotice.text)
            }
        }
        if notice.trimUnsupportedVisible {
            HvfNoticeStrip(identifier: "bridgevm.storage.trim-unsupported",
                           dismiss: { notice.dismissTrimUnsupported() }) {
                Text(HvfDiskSyncNotice.trimText)
            }
        }
    }
}
