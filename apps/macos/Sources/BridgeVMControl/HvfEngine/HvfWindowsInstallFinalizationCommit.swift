import Foundation

extension HvfWindowsInstallFinalization {
    static func commitConfiguration(
        _ journal: inout HvfWindowsInstallFinalizationJournal,
        paths: HvfWindowsInstallFinalizationPaths, faultInjector: FaultInjector
    ) throws {
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
            try advance(&journal, to: .committed, boundary: .committed, paths: paths, faultInjector: faultInjector)
        }
    }
}
