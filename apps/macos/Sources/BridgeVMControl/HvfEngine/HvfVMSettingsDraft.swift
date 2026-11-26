import SwiftUI

/// Pending text only; the existing per-setting session methods own validation and persistence.
@MainActor
final class HvfVMSettingsDraft: ObservableObject {
    let session: HvfEngineSession
    @Published var name: String
    @Published var nameMessage: String?
    private var savedName: String
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
        let initialName = session.config.libraryContext?.config.name ?? ""
        name = initialName
        savedName = initialName
        shareEnabled = session.config.shareHostDir != nil
        host = session.config.shareHostDir ?? ""
        guest = session.config.shareGuestDir ?? "C:\\bridgevm-share"
        forwards = HvfPortForwards.text(session.config.portForwards)
        savedForwards = HvfPortForwards.text(session.config.portForwards)
        savedShare = session.config.shareHostDir.map {
            HvfSharedFolderSelection(hostPath: $0, guestPath: session.config.shareGuestDir ?? "")
        }
    }

    func refreshName() {
        guard let current = session.config.libraryContext?.config.name, current != savedName, name == savedName else { return }
        name = current
        savedName = current
        nameMessage = nil
    }

    @discardableResult func commitName() -> Bool {
        refreshName()
        guard name != savedName else { nameMessage = nil; return true }
        guard let trimmed = VMLibrary.normalizedDisplayName(name) else {
            nameMessage = "Enter a VM name with 1 to \(VMLibrary.maximumVMNameCharacters) characters and no control characters."
            return false
        }
        if let current = session.config.libraryContext?.config.name, current != savedName, current != trimmed {
            savedName = current
            nameMessage = "Another window saved the name “\(current)”. Click Done again to use your edit."
            return false
        }
        nameMessage = session.saveLibraryPreference("VM name", publish: { VMRegistrationWriter.commit($0, to: $1) }) { saved in
            saved.name = trimmed
            saved.displayName = trimmed
        }
        if session.config.libraryContext?.config.name == trimmed {
            name = trimmed
            savedName = trimmed
        }
        return nameMessage == nil
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
        let nameOK = commitName()
        let shareOK = commitShare()
        let forwardsOK = !session.config.virtioNet || commitForwards()
        return nameOK && shareOK && forwardsOK
    }

    func didAppear() { session.openSettingsSheets.insert(ObjectIdentifier(self)) }

    func didDisappear() {
        commitShare()
        if session.config.virtioNet { commitForwards() }
        session.openSettingsSheets.remove(ObjectIdentifier(self))
    }
}
