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
            _ = try resume(journal, paths: paths, faultInjector: faultInjector, secureBootSeeder: secureBootSeeder)
        }
    }

    static func reconcile(
        config: VMConfig, libraryRoot: URL,
        secureBootSeeder: SecureBootSeeder = defaultSecureBootSeeder,
        removeTransaction: (URL) throws -> Void = { try HvfWindowsInstallDurability.durableRemove($0) }
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
            config = try withExtendedLifetime(lock) {
                try resume(loadJournal(paths.journal), paths: paths, faultInjector: { _ in },
                           secureBootSeeder: secureBootSeeder, removeTransaction: removeTransaction)
            }
            return ReconcileResult(config: config, issue: nil)
        } catch let cleanup as HvfWindowsInstallCommittedCleanupFailure {
            return ReconcileResult(config: cleanup.config, issue: cleanup.localizedDescription)
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

}
