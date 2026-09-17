import Foundation

enum VTPMImportedRecoveryCleanupError: LocalizedError {
    case keyMismatch
    var errorDescription: String? { "현재 vTPM 키가 선택한 복구 패키지와 일치하지 않습니다." }
}

extension VTPMIdentityLifecycle {
    func removeImportedRecovery(destinationStableVMID: String, stateDirectory: URL,
        packageURL: URL, recoveryCode: String) throws {
        var expected = try importedRecoveryKey(stateDirectory: stateDirectory,
            packageURL: packageURL, recoveryCode: recoveryCode)
        defer { expected.resetBytes(in: expected.indices) }
        var existing = try keyStore.stateKey(for: destinationStableVMID, allowCreation: false)
        defer { existing.resetBytes(in: existing.indices) }
        guard expected.count == existing.count else { throw VTPMImportedRecoveryCleanupError.keyMismatch }
        var difference: UInt8 = 0
        for index in expected.indices { difference |= expected[index] ^ existing[index] }
        guard difference == 0 else { throw VTPMImportedRecoveryCleanupError.keyMismatch }
        try keyStore.deleteStateKey(for: destinationStableVMID)
    }
}
