import SwiftUI

/// Pending text only; the existing per-setting session methods own validation and persistence.
@MainActor
final class HvfVMSettingsDraft: ObservableObject {
    let session: HvfEngineSession
    @Published var shareEnabled: Bool
    @Published var host: String
    @Published var guest: String
    @Published var forwards: String
    @Published var shareMessage: String?
    @Published var forwardsMessage: String?
    private var savedShare: HvfSharedFolderSelection?
    private var savedForwards: String

    init(session: HvfEngineSession) {
        self.session = session
        shareEnabled = session.config.shareHostDir != nil
        host = session.config.shareHostDir ?? ""
        guest = session.config.shareGuestDir ?? "C:\\bridgevm-share"
        forwards = HvfPortForwards.text(session.config.portForwards)
        savedForwards = HvfPortForwards.text(session.config.portForwards)
        savedShare = session.config.shareHostDir.map {
            HvfSharedFolderSelection(hostPath: $0, guestPath: session.config.shareGuestDir ?? "")
        }
    }

    @discardableResult func commitShare() -> Bool {
        let selection = shareEnabled ? HvfSharedFolderSelection(hostPath: host, guestPath: guest) : nil
        guard selection != savedShare else { shareMessage = nil; return true }
        let result = session.applySharedFolderSelection(selection)
        shareMessage = result.message
        if result.message == nil { savedShare = result.selection }
        return result.message == nil
    }

    @discardableResult func commitForwards() -> Bool {
        guard forwards != savedForwards else { forwardsMessage = nil; return true }
        let result = session.applyPortForwards(forwards)
        forwardsMessage = result.message
        if result.message == nil { forwards = result.text; savedForwards = result.text }
        return result.message == nil
    }

    func finish() -> Bool {
        let shareOK = commitShare()
        let forwardsOK = commitForwards()
        return shareOK && forwardsOK
    }
}
