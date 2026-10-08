import Foundation

/// Final artifacts are committed; an error removing disposable transaction files
/// cannot turn them back into an uninstalled VM or authorize another installer.
struct HvfWindowsInstallCommittedCleanupFailure: LocalizedError {
    let config: VMConfig
    var errorDescription: String? {
        "Windows 설치 결과는 저장했지만 임시 transaction 정리를 확인하지 못했습니다. 설치를 다시 실행하지 말고 라이브러리를 다시 확인하세요."
    }
}

extension HvfWindowsInstallFinalization {
    static func recoveryRequest(_ journal: HvfWindowsInstallFinalizationJournal,
                                paths: HvfWindowsInstallFinalizationPaths) -> URL {
        journal.phase == .committed ? paths.doneRequest
            : (journal.phase < .requestStaged ? paths.pendingRequest : paths.stagedRequest)
    }

    static func finishCommitted(
        _ journal: HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths,
        removeTransaction: (URL) throws -> Void
    ) throws -> VMConfig {
        guard journal.phase == .committed, let diskHash = journal.diskSHA256,
              let varsHash = journal.provisionedVarsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        // Cleanup may have removed staged copies already. Authenticate final
        // artifacts instead; incomplete/corrupt publication still fails closed.
        try verify(paths.finalDisk, bytes: journal.diskBytes, sha256: diskHash)
        try verify(paths.finalVars, bytes: journal.varsBytes, sha256: varsHash)
        try validateReceipt(paths.finalReceipt)
        try validateRequest(paths.doneRequest, expectedSHA256: journal.requestSHA256)
        let config = try loadConfig(paths.config)
        try validateConfig(config, pending: false, paths: paths)
        guard !FileManager.default.fileExists(atPath: paths.pendingRequest.path) else {
            throw HvfWindowsInstallFinalizationError.invalidState("설치 완료 후 대기 요청이 남아 있습니다.")
        }
        try? HvfWindowsInstallStaging.discard(paths)
        do { try removeTransaction(paths.transaction) }
        catch { throw HvfWindowsInstallCommittedCleanupFailure(config: config) }
        return config
    }
}
