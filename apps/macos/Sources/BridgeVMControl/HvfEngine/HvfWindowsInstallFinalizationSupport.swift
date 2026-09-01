import Foundation

extension HvfWindowsInstallFinalization {
    // Schema v1 still decodes because digest fields are optional, but validate
    // refuses to resume it: same-size replacement cannot be disproved safely.
    static let currentJournalSchemaVersion = 2

    static func paths(slug: String, libraryRoot: URL,
                      bundlePath: String) -> HvfWindowsInstallFinalizationPaths {
        HvfWindowsInstallFinalizationPaths(
            libraryRoot: libraryRoot, bundle: URL(fileURLWithPath: bundlePath), slug: slug)
    }

    static func validate(
        journal: HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths
    ) throws {
        guard journal.schemaVersion == currentJournalSchemaVersion,
              journal.slug == paths.slug,
              journal.libraryRoot == HvfWindowsInstallDurability.canonical(paths.libraryRoot),
              journal.bundlePath == HvfWindowsInstallDurability.canonical(paths.bundle),
              journal.sourceDiskPath == "/tmp/bridgevm-appinstall-\(paths.slug)-target.raw",
              journal.sourceVarsPath == "/tmp/bridgevm-appinstall-\(paths.slug)-vars.fd",
              journal.diskBytes > 0, journal.varsBytes > 0,
              journal.requestSHA256.count == 64,
              journal.diskSHA256?.count == 64,
              journal.varsSHA256?.count == 64,
              journal.phase < .secureBootStaged || journal.provisionedVarsSHA256?.count == 64 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        try validatePathIdentity(paths: paths, slug: journal.slug, bundlePath: journal.bundlePath)
    }

    static func validatePathIdentity(
        paths: HvfWindowsInstallFinalizationPaths, slug: String, bundlePath: String
    ) throws {
        guard slug == VMConfig.slugify(slug),
              HvfWindowsInstallDurability.canonical(paths.bundle)
                == HvfWindowsInstallDurability.canonical(URL(fileURLWithPath: bundlePath)) else {
            throw HvfWindowsInstallFinalizationError.unsafePath(bundlePath)
        }
        for url in [paths.libraryRoot, paths.config.deletingLastPathComponent(), paths.bundle,
                    paths.metadata, paths.transaction, paths.config, paths.finalDisk,
                    paths.finalVars, paths.finalReceipt, paths.pendingRequest, paths.doneRequest] {
            try HvfWindowsInstallDurability.refuseSymlink(url)
        }
    }

    static func advance(
        _ journal: inout HvfWindowsInstallFinalizationJournal,
        to phase: HvfWindowsInstallFinalizationJournal.Phase,
        boundary: HvfWindowsInstallFinalizationBoundary,
        paths: HvfWindowsInstallFinalizationPaths,
        faultInjector: FaultInjector
    ) throws {
        journal.phase = phase
        try writeJournal(journal, to: paths.journal)
        try faultInjector(boundary)
    }

    static func stage(
        _ source: URL, to destination: URL, bytes: UInt64, sha256: String
    ) throws {
        try verify(source, bytes: bytes, sha256: sha256)
        try HvfWindowsInstallDurability.durableCloneOrCopy(from: source, to: destination)
        try verify(destination, bytes: bytes, sha256: sha256)
    }

    static func publish(
        _ staged: URL, to final: URL, bytes: UInt64, sha256: String,
        phase: HvfWindowsInstallFinalizationJournal.Phase,
        boundary: HvfWindowsInstallFinalizationBoundary,
        journal: inout HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths,
        faultInjector: FaultInjector
    ) throws {
        try verify(staged, bytes: bytes, sha256: sha256)
        if journal.phase < phase {
            try HvfWindowsInstallDurability.durableCloneOrCopy(from: staged, to: final)
            try verify(final, bytes: bytes, sha256: sha256)
            try advance(&journal, to: phase, boundary: boundary,
                        paths: paths, faultInjector: faultInjector)
        } else { try verify(final, bytes: bytes, sha256: sha256) }
    }

    static func publishFile(
        _ staged: URL, to final: URL,
        phase: HvfWindowsInstallFinalizationJournal.Phase,
        boundary: HvfWindowsInstallFinalizationBoundary,
        journal: inout HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths,
        faultInjector: FaultInjector,
        validator: (URL) throws -> Void
    ) throws {
        if journal.phase < phase {
            try HvfWindowsInstallDurability.durableCloneOrCopy(from: staged, to: final)
            try validator(final)
            try advance(&journal, to: phase, boundary: boundary,
                        paths: paths, faultInjector: faultInjector)
        } else if (try? validator(final)) == nil {
            try HvfWindowsInstallDurability.durableCloneOrCopy(from: staged, to: final)
            try validator(final)
        }
    }

    static func ensureAuxiliaryFiles(
        paths: HvfWindowsInstallFinalizationPaths, installLog: URL?, finalLog: URL?
    ) throws {
        if !FileManager.default.fileExists(atPath: paths.control.path) {
            try HvfWindowsInstallDurability.durableWrite(Data(), to: paths.control)
        } else { try HvfWindowsInstallDurability.refuseSymlink(paths.control) }
        if let installLog, let finalLog,
           FileManager.default.fileExists(atPath: installLog.path) {
            try? HvfWindowsInstallDurability.durableCloneOrCopy(from: installLog, to: finalLog)
        }
    }

    static func verify(_ url: URL, bytes: UInt64, sha256: String) throws {
        try HvfWindowsInstallFinalizationIdentity.verify(url, bytes: bytes, sha256: sha256)
    }

    static func verifyProvisionedVars(
        _ journal: HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths
    ) throws {
        guard let sha256 = journal.provisionedVarsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        try verify(paths.stagedProvisionedVars, bytes: journal.varsBytes, sha256: sha256)
    }

    static func validateReceipt(_ url: URL) throws {
        let data = try HvfWindowsInstallDurability.readRegularFile(
            url, maximumBytes: VMLibrary.maximumConfigBytes)
        _ = try JSONDecoder().decode(HvfSecureBootProvisioningReceipt.self, from: data)
    }

    static func validateRequest(_ url: URL, expectedSHA256: String? = nil) throws {
        let data = try HvfWindowsInstallDurability.readRegularFile(
            url, maximumBytes: VMLibrary.maximumConfigBytes)
        _ = try JSONDecoder().decode(HvfWindowsInstallRequest.self, from: data)
        if let expectedSHA256,
           HvfWindowsInstallCacheIdentity.sha256File(url.path) != expectedSHA256 {
            throw HvfWindowsInstallFinalizationError.invalidState("설치 요청 digest가 journal과 다릅니다.")
        }
    }

    static func validateConfig(
        _ config: VMConfig, pending: Bool, paths: HvfWindowsInstallFinalizationPaths
    ) throws {
        guard config.slug == paths.slug, config.installPending == pending,
              HvfWindowsInstallDurability.canonical(URL(fileURLWithPath: config.bundlePath))
                == HvfWindowsInstallDurability.canonical(paths.bundle) else {
            throw HvfWindowsInstallFinalizationError.invalidState("VM 설정이 transaction identity와 다릅니다.")
        }
    }

    static func loadConfig(_ url: URL) throws -> VMConfig {
        let data = try HvfWindowsInstallDurability.readRegularFile(
            url, maximumBytes: VMLibrary.maximumConfigBytes)
        return try JSONDecoder().decode(VMConfig.self, from: data)
    }

    static func persistConfig(_ config: VMConfig, to url: URL) throws {
        var config = config
        config.id = config.slug
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(config)
        guard data.count <= VMLibrary.maximumConfigBytes else {
            throw HvfWindowsInstallFinalizationError.invalidState("VM 설정이 허용 크기를 초과했습니다.")
        }
        try HvfWindowsInstallDurability.durableWrite(data, to: url)
    }

    static func loadJournal(_ url: URL) throws -> HvfWindowsInstallFinalizationJournal {
        let data = try HvfWindowsInstallDurability.readRegularFile(
            url, maximumBytes: VMLibrary.maximumConfigBytes)
        return try JSONDecoder().decode(HvfWindowsInstallFinalizationJournal.self, from: data)
    }

    static func writeJournal(
        _ journal: HvfWindowsInstallFinalizationJournal, to url: URL
    ) throws {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try HvfWindowsInstallDurability.durableWrite(try encoder.encode(journal), to: url)
    }

    static func defaultSecureBootSeeder(_ varsPath: String, _ diskPath: String) throws -> Data {
        let receipt = try HvfWindowsBootSeed.seedFile(varsPath: varsPath, diskPath: diskPath)
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(receipt)
    }

    static func restoreLegacyPendingRequest(
        config: VMConfig, paths: HvfWindowsInstallFinalizationPaths
    ) -> ReconcileResult {
        guard config.installPending == true,
              !FileManager.default.fileExists(atPath: paths.pendingRequest.path),
              FileManager.default.fileExists(atPath: paths.doneRequest.path) else {
            return ReconcileResult(config: config, issue: nil)
        }
        do {
            try validateRequest(paths.doneRequest)
            try HvfWindowsInstallDurability.durableCloneOrCopy(
                from: paths.doneRequest, to: paths.pendingRequest)
            return ReconcileResult(
                config: config,
                issue: "이전 설치 완료 중단 기록을 재시도 가능한 대기 요청으로 복구했습니다.")
        } catch {
            return ReconcileResult(
                config: config,
                issue: "중단된 Windows 설치 요청을 복구하지 못해 실행을 차단했습니다: \(error.localizedDescription)")
        }
    }
}
