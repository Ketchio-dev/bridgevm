import Foundation

enum HvfWindowsInstallFinalizationBoundary: String, CaseIterable {
    case prepared, diskStaged, varsStaged, secureBootStaged, requestStaged, configStaged
    case diskPublished, varsPublished, secureBootPublished, requestPublished
    case configPublished, pendingRemoved, committed
}

struct HvfWindowsInstallFinalizationJournal: Codable, Equatable {
    enum Phase: Int, Codable, Comparable {
        case prepared, diskStaged, varsStaged, secureBootStaged, requestStaged, configStaged
        case diskPublished, varsPublished, secureBootPublished, requestPublished
        case configPublished, pendingRemoved, committed

        static func < (lhs: Self, rhs: Self) -> Bool { lhs.rawValue < rhs.rawValue }
    }

    var schemaVersion: Int
    var transactionID: String
    var phase: Phase
    var slug: String
    var libraryRoot: String
    var bundlePath: String
    var sourceDiskPath, sourceVarsPath: String
    var diskBytes, varsBytes: UInt64
    var requestSHA256: String
    var diskSHA256, varsSHA256, provisionedVarsSHA256: String?
}

struct HvfWindowsInstallFinalizationPaths {
    let libraryRoot: URL
    let bundle: URL
    let slug: String

    var config: URL { libraryRoot.appendingPathComponent(slug).appendingPathComponent("vm.json") }
    var metadata: URL { bundle.appendingPathComponent("metadata", isDirectory: true) }
    var transaction: URL { metadata.appendingPathComponent("hvf-install-finalization", isDirectory: true) }
    var journal: URL { transaction.appendingPathComponent("journal.json") }
    var lock: URL { transaction.appendingPathComponent("lock") }
    var stagedDisk: URL { transaction.appendingPathComponent("disk.raw") }
    var stagedVars: URL { transaction.appendingPathComponent("vars.fd") }
    var stagedProvisionedVars: URL { transaction.appendingPathComponent("vars-provisioned.fd") }
    var stagedReceipt: URL { transaction.appendingPathComponent("secure-boot-provisioning.json") }
    var stagedRequest: URL { transaction.appendingPathComponent("hvf-install-done.json") }
    var stagedConfig: URL { transaction.appendingPathComponent("vm.json") }
    var finalDisk: URL { bundle.appendingPathComponent("disks/hvf-target.raw") }
    var finalVars: URL { metadata.appendingPathComponent("hvf-vars.fd") }
    var finalReceipt: URL { metadata.appendingPathComponent("secure-boot-provisioning.json") }
    var pendingRequest: URL { bundle.appendingPathComponent(HvfWindowsInstallRequest.fileName) }
    var doneRequest: URL { bundle.appendingPathComponent(HvfWindowsInstallRequest.doneFileName) }
    var control: URL { metadata.appendingPathComponent("hvf.ctl") }
}

enum HvfWindowsInstallFinalizationError: LocalizedError {
    case invalidState(String)
    case unsupportedJournal
    case unsafePath(String)
    case missingArtifact(String)
    case transactionBusy

    var errorDescription: String? {
        switch self {
        case let .invalidState(message): return message
        case .unsupportedJournal: return "지원하지 않거나 손상된 Windows 설치 journal입니다."
        case let .unsafePath(path): return "심볼릭 링크 또는 범위 밖 경로를 거부했습니다: \(path)"
        case let .missingArtifact(path): return "Windows 설치 transaction 산출물이 없습니다: \(path)"
        case .transactionBusy: return "다른 Windows 설치 완료 transaction이 실행 중입니다."
        }
    }
}
