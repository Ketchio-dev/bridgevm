import Foundation

enum HvfWindowsInstallFinalization {
    typealias FaultInjector = (HvfWindowsInstallFinalizationBoundary) throws -> Void
    typealias SecureBootSeeder = (_ varsPath: String, _ diskPath: String) throws -> Data

    struct ReconcileResult {
        var config: VMConfig
        var issue: String?
    }

    static func finalize(
        plan: HvfWindowsInstallPlan,
        faultInjector: FaultInjector = { _ in },
        secureBootSeeder: SecureBootSeeder = defaultSecureBootSeeder
    ) throws {
        let paths = paths(slug: plan.slug, libraryRoot: plan.libraryRoot,
                          bundlePath: plan.bundlePath)
        try HvfWindowsInstallDurability.refuseSymlink(paths.bundle)
        try HvfWindowsInstallDurability.refuseSymlink(paths.metadata)
        try HvfWindowsInstallDurability.ensureDirectory(paths.transaction)
        let lock = try HvfWindowsInstallDurability.TransactionLock(url: paths.lock, nonBlocking: true)
        try withExtendedLifetime(lock) {
            let journal: HvfWindowsInstallFinalizationJournal
            if FileManager.default.fileExists(atPath: paths.journal.path) {
                journal = try loadJournal(paths.journal)
            } else {
                journal = try begin(plan: plan, paths: paths)
                try faultInjector(.prepared)
            }
            try resume(journal, paths: paths, faultInjector: faultInjector,
                       secureBootSeeder: secureBootSeeder, installLog: URL(
                        fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log"),
                       finalLog: URL(fileURLWithPath: plan.bundleInstallLogPath))
        }
    }

    static func reconcile(
        config: VMConfig,
        libraryRoot: URL,
        secureBootSeeder: SecureBootSeeder = defaultSecureBootSeeder
    ) -> ReconcileResult {
        var config = config
        let paths = paths(slug: config.slug, libraryRoot: libraryRoot,
                          bundlePath: config.bundlePath)
        guard FileManager.default.fileExists(atPath: paths.journal.path) else {
            return restoreLegacyPendingRequest(config: config, paths: paths)
        }
        do {
            try HvfWindowsInstallDurability.refuseSymlink(paths.bundle)
            try HvfWindowsInstallDurability.refuseSymlink(paths.metadata)
            try HvfWindowsInstallDurability.refuseSymlink(paths.transaction)
            let lock = try HvfWindowsInstallDurability.TransactionLock(
                url: paths.lock, nonBlocking: true)
            try withExtendedLifetime(lock) {
                let journal = try loadJournal(paths.journal)
                try resume(journal, paths: paths, faultInjector: { _ in },
                           secureBootSeeder: secureBootSeeder,
                           installLog: nil, finalLog: nil)
            }
            config = try loadConfig(paths.config)
            return ReconcileResult(config: config, issue: nil)
        } catch HvfWindowsInstallFinalizationError.transactionBusy {
            config.installPending = true
            return ReconcileResult(config: config, issue: nil)
        } catch {
            config.installPending = true
            try? persistConfig(config, to: paths.config)
            return ReconcileResult(
                config: config,
                issue: "Windows 설치 완료 transaction을 복구하지 못해 실행을 차단했습니다: \(error.localizedDescription)")
        }
    }

    private static func begin(
        plan: HvfWindowsInstallPlan,
        paths: HvfWindowsInstallFinalizationPaths
    ) throws -> HvfWindowsInstallFinalizationJournal {
        try validatePathIdentity(paths: paths, slug: plan.slug, bundlePath: plan.bundlePath)
        let config = try loadConfig(paths.config)
        guard config.slug == plan.slug,
              HvfWindowsInstallDurability.canonical(URL(fileURLWithPath: config.bundlePath))
                == HvfWindowsInstallDurability.canonical(paths.bundle),
              config.installPending == true else {
            throw HvfWindowsInstallFinalizationError.invalidState(
                "설치 대기 중인 동일 VM 설정을 찾을 수 없습니다.")
        }
        let requestSnapshot = try HvfWindowsInstallRequestSnapshot.load(paths.pendingRequest)
        guard requestSnapshot.request == plan.request else {
            throw HvfWindowsInstallFinalizationError.invalidState("저장된 설치 요청이 실행 계획과 다릅니다.")
        }
        let sourceDisk = URL(fileURLWithPath: plan.tmpTargetPath)
        let sourceVars = URL(fileURLWithPath: plan.tmpVarsPath)
        let diskIdentity = try HvfWindowsInstallFinalizationIdentity.seal(sourceDisk)
        let varsIdentity = try HvfWindowsInstallFinalizationIdentity.seal(sourceVars)
        let journal = HvfWindowsInstallFinalizationJournal(
            schemaVersion: currentJournalSchemaVersion,
            transactionID: UUID().uuidString, phase: .prepared,
            slug: plan.slug,
            libraryRoot: HvfWindowsInstallDurability.canonical(plan.libraryRoot),
            bundlePath: HvfWindowsInstallDurability.canonical(paths.bundle),
            sourceDiskPath: sourceDisk.path, sourceVarsPath: sourceVars.path,
            diskBytes: diskIdentity.bytes, varsBytes: varsIdentity.bytes,
            requestSHA256: requestSnapshot.sha256,
            diskSHA256: diskIdentity.sha256, varsSHA256: varsIdentity.sha256,
            provisionedVarsSHA256: nil)
        try HvfWindowsInstallDurability.ensureDirectory(paths.transaction)
        try writeJournal(journal, to: paths.journal)
        return journal
    }
}
