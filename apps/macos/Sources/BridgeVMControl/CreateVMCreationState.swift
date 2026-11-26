import Foundation

/// A published registration cannot be rolled back or retried as a fresh creation.
struct CreateVMCreationState {
    private(set) var published: VMConfig?
    private(set) var publicationWarning = ""
    var recoveryFailureMessage: String {
        [publicationWarning, "Couldn’t load the saved VM. Your files are safe. Choose Load Saved VM to try again."].filter { !$0.isEmpty }.joined(separator: "\n")
    }
    var permitsCreation: Bool { published == nil }

    mutating func complete(
        _ outcome: HVFCreationOutcome?, adopt: (VMConfig) -> Bool
    ) -> (dismiss: Bool, code: String, message: String) {
        switch outcome {
        case .created(let config):
            if adopt(config) {
                if let notice = HvfWindowsImportNotice.read(bundlePath: config.bundlePath) {
                    published = config
                    return (false, "", notice)
                }
                return (true, "", "")
            }
            published = config
            return (false, "library-publication-failed", "Your VM files were saved, but the VM didn’t appear in the library. Choose Load Saved VM to reopen it.")
        case .publishedButUnsynced(let config, let message):
            published = config
            publicationWarning = message
            return (false, "vm-publication-unconfirmed", message)
        case .refused(let message):
            return (false, "hvf-import-invalid", message)
        case nil:
            return (false, "vm-materialization-failed", "Couldn’t create the VM or save it to the library.")
        }
    }

    /// Only read/adopt the saved registration. This does not confirm its prior fsync.
    mutating func recover(adopt: (VMConfig) -> Bool) -> Bool {
        guard let published else { return false }
        guard adopt(published) else { return false }
        self.published = nil
        publicationWarning = ""
        return true
    }
}
