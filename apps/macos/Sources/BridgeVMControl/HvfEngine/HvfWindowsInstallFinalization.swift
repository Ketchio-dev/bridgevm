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
            // A journal left by an earlier run is re-verified; one sealed just now is not re-hashed.
            let recovering = FileManager.default.fileExists(atPath: paths.journal.path)
            if recovering {
                journal = try loadJournal(paths.journal)
            } else {
                journal = try begin(plan: plan, paths: paths)
                try faultInjector(.prepared)
            }
            _ = try resume(journal, paths: paths, recovering: recovering,
                           faultInjector: faultInjector, secureBootSeeder: secureBootSeeder)
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
                try resume(loadJournal(paths.journal), paths: paths, recovering: true, faultInjector: { _ in },
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
                issue: "Windows can’t start until setup recovers the interrupted installation. \(error.localizedDescription)")
        }
    }

}

extension HvfWindowsInstallFinalization {
    static func commitConfiguration(
        _ journal: inout HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths, faultInjector: FaultInjector
    ) throws {
        if journal.phase < .configPublished {
            try HvfWindowsInstallDurability.durableCloneOrCopy(from: paths.stagedConfig, to: paths.config)
            let committed = try loadConfig(paths.config)
            guard committed.installPending == false else {
                throw HvfWindowsInstallFinalizationError.invalidState("Couldn’t publish the saved Windows installation settings.")
            }
            try advance(&journal, to: .configPublished, boundary: .configPublished,
                        paths: paths, faultInjector: faultInjector)
        } else if try loadConfig(paths.config).installPending != false {
            try HvfWindowsInstallDurability.durableCloneOrCopy(from: paths.stagedConfig, to: paths.config)
        }
        if journal.phase < .pendingRemoved {
            try HvfWindowsInstallDurability.durableRemove(paths.pendingRequest)
            try advance(&journal, to: .pendingRemoved, boundary: .pendingRemoved,
                        paths: paths, faultInjector: faultInjector)
        } else if FileManager.default.fileExists(atPath: paths.pendingRequest.path) {
            try HvfWindowsInstallDurability.durableRemove(paths.pendingRequest)
        }
        if journal.phase < .committed {
            try advance(&journal, to: .committed, boundary: .committed, paths: paths, faultInjector: faultInjector)
        }
    }
}

extension HvfWindowsInstallFinalization {
    static func begin(
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
                "Couldn’t find the matching VM with a pending installation.")
        }
        let requestSnapshot = try HvfWindowsInstallRequestSnapshot.load(paths.pendingRequest)
        guard requestSnapshot.request == plan.request else {
            throw HvfWindowsInstallFinalizationError.invalidState("The saved installation request differs from the installation plan.")
        }
        let sourceDisk = paths.stagingDisk
        let sourceVars = paths.stagingVars
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

extension HvfWindowsInstallFinalization {
    /// `recovering` re-verifies every sealed digest after a crash. A run that sealed the
    /// pair itself hashes the disk once: every copy after the seal is an APFS clone.
    static func resume(
        _ stored: HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths, recovering: Bool,
        faultInjector: FaultInjector, secureBootSeeder: SecureBootSeeder,
        removeTransaction: (URL) throws -> Void = { try HvfWindowsInstallDurability.durableRemove($0) }
    ) throws -> VMConfig {
        var journal = stored
        try validate(journal: journal, paths: paths)
        if journal.phase == .committed {
            return try finishCommitted(journal, paths: paths, verifying: true, removeTransaction: removeTransaction)
        }
        guard let diskSHA256 = journal.diskSHA256,
              let varsSHA256 = journal.varsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        let sourceDisk = URL(fileURLWithPath: journal.sourceDiskPath)
        let sourceVars = URL(fileURLWithPath: journal.sourceVarsPath)

        if journal.phase < .diskStaged {
            try stage(sourceDisk, to: paths.stagedDisk, bytes: journal.diskBytes,
                      sha256: diskSHA256, verifying: recovering)
            try advance(&journal, to: .diskStaged, boundary: .diskStaged,
                        paths: paths, faultInjector: faultInjector)
            try? HvfWindowsInstallStaging.consume(sourceDisk, paths: paths)
        } else { try verify(paths.stagedDisk, bytes: journal.diskBytes, sha256: diskSHA256) }
        if journal.phase < .varsStaged {
            try stage(sourceVars, to: paths.stagedVars, bytes: journal.varsBytes,
                      sha256: varsSHA256, verifying: recovering)
            try advance(&journal, to: .varsStaged, boundary: .varsStaged,
                        paths: paths, faultInjector: faultInjector)
            try? HvfWindowsInstallStaging.consume(sourceVars, paths: paths)
        } else { try verify(paths.stagedVars, bytes: journal.varsBytes, sha256: varsSHA256) }
        if journal.phase < .secureBootStaged {
            try HvfWindowsInstallDurability.durableCloneOrCopy(
                from: paths.stagedVars, to: paths.stagedProvisionedVars)
            let receipt = try secureBootSeeder(
                paths.stagedProvisionedVars.path, paths.stagedDisk.path)
            _ = try JSONDecoder().decode(HvfSecureBootProvisioningReceipt.self, from: receipt)
            try HvfWindowsInstallDurability.syncFile(paths.stagedProvisionedVars)
            let provisioned = try HvfWindowsInstallFinalizationIdentity.seal(
                paths.stagedProvisionedVars)
            guard provisioned.bytes == journal.varsBytes else {
                throw HvfWindowsInstallFinalizationError.invalidState(
                    "The firmware settings file changed size while applying Secure Boot settings.")
            }
            journal.provisionedVarsSHA256 = provisioned.sha256
            try HvfWindowsInstallDurability.durableWrite(receipt, to: paths.stagedReceipt)
            try advance(&journal, to: .secureBootStaged, boundary: .secureBootStaged,
                        paths: paths, faultInjector: faultInjector)
        } else {
            try validateReceipt(paths.stagedReceipt)
            try verifyProvisionedVars(journal, paths: paths)
        }
        if journal.phase < .requestStaged {
            let snapshot = try HvfWindowsInstallRequestSnapshot.load(
                paths.pendingRequest, expectedSHA256: journal.requestSHA256)
            try HvfWindowsInstallDurability.durableWrite(snapshot.data, to: paths.stagedRequest)
            try advance(&journal, to: .requestStaged, boundary: .requestStaged,
                        paths: paths, faultInjector: faultInjector)
        } else { try validateRequest(paths.stagedRequest, expectedSHA256: journal.requestSHA256) }
        if journal.phase < .configStaged {
            var config = try loadConfig(paths.config)
            try validateConfig(config, pending: true, paths: paths)
            config.installPending = false
            try persistConfig(config, to: paths.stagedConfig)
            try ensureAuxiliaryFiles(paths: paths)
            try advance(&journal, to: .configStaged, boundary: .configStaged,
                        paths: paths, faultInjector: faultInjector)
        } else {
            try validateConfig(try loadConfig(paths.stagedConfig), pending: false, paths: paths)
            try ensureAuxiliaryFiles(paths: paths)
        }

        try publish(paths.stagedDisk, to: paths.finalDisk, bytes: journal.diskBytes,
                    sha256: diskSHA256, verifying: recovering,
                    phase: .diskPublished, boundary: .diskPublished,
                    journal: &journal, paths: paths, faultInjector: faultInjector)
        guard let provisionedVarsSHA256 = journal.provisionedVarsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        try publish(paths.stagedProvisionedVars, to: paths.finalVars, bytes: journal.varsBytes,
                    sha256: provisionedVarsSHA256, verifying: recovering,
                    phase: .varsPublished, boundary: .varsPublished,
                    journal: &journal, paths: paths, faultInjector: faultInjector)
        try publishFile(paths.stagedReceipt, to: paths.finalReceipt,
                        phase: .secureBootPublished, boundary: .secureBootPublished,
                        journal: &journal, paths: paths, faultInjector: faultInjector,
                        validator: validateReceipt)
        let requestSHA256 = journal.requestSHA256
        try publishFile(paths.stagedRequest, to: paths.doneRequest,
                        phase: .requestPublished, boundary: .requestPublished,
                        journal: &journal, paths: paths, faultInjector: faultInjector,
                        validator: { try validateRequest($0, expectedSHA256: requestSHA256) })
        try commitConfiguration(&journal, paths: paths, faultInjector: faultInjector)
        return try finishCommitted(journal, paths: paths, verifying: recovering,
                                   removeTransaction: removeTransaction)
    }
}
