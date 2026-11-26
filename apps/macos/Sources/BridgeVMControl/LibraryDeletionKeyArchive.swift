import Foundation

struct LibraryDeletionResult: Equatable {
    let outcome: LibraryDeletionOutcome
    var warning: String? = nil
}

typealias LibraryDeletionOperation = @Sendable (VMConfig, URL, () -> Bool) -> LibraryDeletionResult

/// Library deletion, then retirement of the removed VM's vTPM key (keyed by its
/// slug). Only an HVF entry whose managed bundle was removed retires the key; a
/// registration-only removal leaves a bundle whose vTPM state still needs it.
/// A failed retirement is a warning: it never fails or reverts the deletion.
enum LibraryDeletionKeyArchive {
    static let live: LibraryDeletionOperation = {
        LibraryDeletionKeyArchive.perform($0, rootURL: $1, isRunning: $2, keyStore: KeychainVTPMStateKeyStore())
    }

    static func perform(_ config: VMConfig, rootURL: URL, isRunning: () -> Bool, keyStore: VTPMStateKeyManaging,
                        delete: (VMConfig, URL) -> Bool = LibraryDeletionProtection.delete) -> LibraryDeletionResult {
        // Decided immediately before removal: afterwards the entry is gone.
        var impact = VMLibraryDeletionImpact.registrationOnly
        let outcome = LibraryDeletionAction.perform(isRunning: isRunning) {
            impact = VMLibrary.deletionImpact(for: config, rootURL: rootURL)
            return delete(config, rootURL)
        }
        guard outcome == .deleted, config.engineKind == .hvfEngine, impact == .managedBundleDeleted else {
            return LibraryDeletionResult(outcome: outcome)
        }
        let archive = VTPMIdentityLifecycle(keyStore: keyStore).archiveDeletedIdentity(stableVMID: config.slug)
        guard case let .failed(reason) = archive else { return LibraryDeletionResult(outcome: .deleted) }
        NSLog("Could not archive the deleted VM’s vTPM key: %@", reason)
        return LibraryDeletionResult(outcome: .deleted, warning: "BridgeVM deleted \(config.name) but could not archive its vTPM key. "
            + "The key may still be attached to VM ID '\(config.slug)'. Reusing that ID could reuse the key or "
            + "prevent a vTPM recovery import. Choose a different VM ID until the old key is archived.")
    }
}
