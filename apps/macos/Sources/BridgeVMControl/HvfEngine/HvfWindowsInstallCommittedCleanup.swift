import Foundation

/// Final artifacts are committed; an error removing disposable transaction files
/// cannot turn them back into an uninstalled VM or authorize another installer.
struct HvfWindowsInstallCommittedCleanupFailure: LocalizedError {
    let config: VMConfig
    var errorDescription: String? {
        "Windows is installed. Press Command-K in the library window and choose Refresh Library to finish cleaning up temporary files. Keep the VM files and avoid running the installer again."
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
        paths: HvfWindowsInstallFinalizationPaths, verifying: Bool,
        removeTransaction: (URL) throws -> Void
    ) throws -> VMConfig {
        guard journal.phase == .committed, let diskHash = journal.diskSHA256,
              let varsHash = journal.provisionedVarsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        // After a crash, cleanup may have removed staged copies already. Authenticate
        // final artifacts instead; incomplete/corrupt publication still fails closed.
        if verifying {
            try verify(paths.finalDisk, bytes: journal.diskBytes, sha256: diskHash)
            try verify(paths.finalVars, bytes: journal.varsBytes, sha256: varsHash)
        }
        try validateReceipt(paths.finalReceipt)
        try validateRequest(paths.doneRequest, expectedSHA256: journal.requestSHA256)
        let config = try loadConfig(paths.config)
        try validateConfig(config, pending: false, paths: paths)
        guard !FileManager.default.fileExists(atPath: paths.pendingRequest.path) else {
            throw HvfWindowsInstallFinalizationError.invalidState("A pending request remains after the Windows installation was saved.")
        }
        try? HvfWindowsInstallStaging.discard(paths)
        do { try removeTransaction(paths.transaction) }
        catch { throw HvfWindowsInstallCommittedCleanupFailure(config: config) }
        return config
    }
}
