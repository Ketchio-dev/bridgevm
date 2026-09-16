import Foundation

extension HvfWindowsInstallFinalization {
    static func resume(
        _ stored: HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths,
        faultInjector: FaultInjector,
        secureBootSeeder: SecureBootSeeder,
        installLog: URL?,
        finalLog: URL?
    ) throws {
        var journal = stored
        try validate(journal: journal, paths: paths)
        guard let diskSHA256 = journal.diskSHA256,
              let varsSHA256 = journal.varsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        let sourceDisk = URL(fileURLWithPath: journal.sourceDiskPath)
        let sourceVars = URL(fileURLWithPath: journal.sourceVarsPath)

        if journal.phase < .diskStaged {
            try stage(sourceDisk, to: paths.stagedDisk, bytes: journal.diskBytes,
                      sha256: diskSHA256)
            try advance(&journal, to: .diskStaged, boundary: .diskStaged,
                        paths: paths, faultInjector: faultInjector)
            try? HvfWindowsInstallDurability.durableRemove(sourceDisk)
        } else { try verify(paths.stagedDisk, bytes: journal.diskBytes, sha256: diskSHA256) }
        if journal.phase < .varsStaged {
            try stage(sourceVars, to: paths.stagedVars, bytes: journal.varsBytes,
                      sha256: varsSHA256)
            try advance(&journal, to: .varsStaged, boundary: .varsStaged,
                        paths: paths, faultInjector: faultInjector)
            try? HvfWindowsInstallDurability.durableRemove(sourceVars)
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
                    "Secure Boot 적용 중 vars 크기가 변경되었습니다.")
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
            try ensureAuxiliaryFiles(paths: paths, installLog: installLog, finalLog: finalLog)
            try advance(&journal, to: .configStaged, boundary: .configStaged,
                        paths: paths, faultInjector: faultInjector)
        } else {
            try validateConfig(try loadConfig(paths.stagedConfig), pending: false, paths: paths)
            try ensureAuxiliaryFiles(paths: paths, installLog: installLog, finalLog: finalLog)
        }

        try publish(paths.stagedDisk, to: paths.finalDisk, bytes: journal.diskBytes,
                    sha256: diskSHA256,
                    phase: .diskPublished, boundary: .diskPublished,
                    journal: &journal, paths: paths, faultInjector: faultInjector)
        guard let provisionedVarsSHA256 = journal.provisionedVarsSHA256 else {
            throw HvfWindowsInstallFinalizationError.unsupportedJournal
        }
        try publish(paths.stagedProvisionedVars, to: paths.finalVars, bytes: journal.varsBytes,
                    sha256: provisionedVarsSHA256,
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
        if journal.phase < .configPublished {
            try HvfWindowsInstallDurability.durableCloneOrCopy(from: paths.stagedConfig, to: paths.config)
            let committed = try loadConfig(paths.config)
            guard committed.installPending == false else {
                throw HvfWindowsInstallFinalizationError.invalidState("설치 완료 설정을 공개하지 못했습니다.")
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
            try advance(&journal, to: .committed, boundary: .committed,
                        paths: paths, faultInjector: faultInjector)
        }
        try HvfWindowsInstallDurability.durableRemove(paths.transaction)
    }
}
