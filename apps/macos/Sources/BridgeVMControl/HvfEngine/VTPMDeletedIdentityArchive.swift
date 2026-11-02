import Foundation

/// `failed` never removes the active key without a stored archive copy; when the
/// active-key removal itself fails, reset's rollback restores it first.
enum VTPMDeletedIdentityArchive: Equatable {
    case noKey
    case archived(keyID: String)
    case failed(String)
}

extension VTPMIdentityLifecycle {
    /// A deleted managed bundle's key must not stay under its stable ID: a later
    /// VM with that ID would silently adopt it, and an imported recovery into the
    /// ID would be refused. Key material is never destroyed: the active key is
    /// copied to `<id>.deleted.<suffix>` (reset's suffix format), then removed.
    func archiveDeletedIdentity(stableVMID: String, now: Date = Date(),
                                nonce: UUID = UUID()) -> VTPMDeletedIdentityArchive {
        var oldKey: Data
        do {
            oldKey = try keyStore.stateKey(for: stableVMID, allowCreation: false)
        } catch VTPMStateSecurityError.missingKeyForExistingState {
            return .noKey
        } catch {
            return .failed(error.localizedDescription)
        }
        defer { oldKey.resetBytes(in: oldKey.indices) }
        let suffix = "\(Int(now.timeIntervalSince1970))-\(nonce.uuidString.lowercased().prefix(8))"
        let archivedKeyID = "\(stableVMID).deleted.\(suffix)"
        do {
            try keyStore.replaceStateKey(oldKey, for: archivedKeyID)
        } catch {
            return .failed(error.localizedDescription)
        }
        do {
            try keyStore.deleteStateKey(for: stableVMID)
        } catch {
            // Drop the copy only once the active key is confirmed rewritten.
            do {
                try keyStore.replaceStateKey(oldKey, for: stableVMID)
                try? keyStore.deleteStateKey(for: archivedKeyID)
            } catch {}
            return .failed(error.localizedDescription)
        }
        return .archived(keyID: archivedKeyID)
    }
}
