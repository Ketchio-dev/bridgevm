import Foundation

enum VTPMImportedRecoveryError: LocalizedError, Equatable {
    case destinationIdentityExists(String)
    var errorDescription: String? {
        switch self {
        case .destinationIdentityExists(let stableID):
            return "대상 VM ID에 이미 vTPM 키가 있습니다: \(stableID)"
        }
    }
}

extension VTPMIdentityLifecycle {
    /// Authenticates source recovery material and installs the state key under
    /// a new imported VM ID without overwriting an existing destination key.
    func restoreImportedRecovery(destinationStableVMID: String, stateDirectory: URL,
        packageURL: URL, recoveryCode: String) throws {
        do {
            var existing = try keyStore.stateKey(for: destinationStableVMID, allowCreation: false)
            existing.resetBytes(in: existing.indices)
            throw VTPMImportedRecoveryError.destinationIdentityExists(destinationStableVMID)
        } catch VTPMStateSecurityError.missingKeyForExistingState {}
        var stateKey = try importedRecoveryKey(stateDirectory: stateDirectory,
            packageURL: packageURL, recoveryCode: recoveryCode)
        defer { stateKey.resetBytes(in: stateKey.indices) }
        try keyStore.replaceStateKey(stateKey, for: destinationStableVMID)
    }
}
