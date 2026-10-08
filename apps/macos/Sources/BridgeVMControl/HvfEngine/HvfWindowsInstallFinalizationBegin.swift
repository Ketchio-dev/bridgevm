import Foundation

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
                "설치 대기 중인 동일 VM 설정을 찾을 수 없습니다.")
        }
        let requestSnapshot = try HvfWindowsInstallRequestSnapshot.load(paths.pendingRequest)
        guard requestSnapshot.request == plan.request else {
            throw HvfWindowsInstallFinalizationError.invalidState("저장된 설치 요청이 실행 계획과 다릅니다.")
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
