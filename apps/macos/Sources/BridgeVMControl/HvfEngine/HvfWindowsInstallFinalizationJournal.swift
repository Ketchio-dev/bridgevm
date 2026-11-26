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
        case .unsupportedJournal: return "The Windows installation recovery record is damaged or unsupported."
        case let .unsafePath(path): return "Rejected a symbolic link or a path outside the installation folder: \(path)"
        case let .missingArtifact(path): return "A file needed to recover the Windows installation is missing: \(path)"
        case .transactionBusy: return "Another task is finishing this Windows installation."
        }
    }
}
