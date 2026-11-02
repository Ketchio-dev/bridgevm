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
        return LibraryDeletionResult(outcome: .deleted, warning: "\(config.name)을(를) 삭제했지만 vTPM 키를 보관하지 못했습니다. "
            + "기존 키가 VM ID '\(config.slug)'에 남아 있을 수 있어 같은 ID의 새 VM이 이 키를 다시 쓰거나 "
            + "같은 ID로의 vTPM 복구 가져오기가 거부될 수 있습니다. 원인: \(reason)")
    }
}
