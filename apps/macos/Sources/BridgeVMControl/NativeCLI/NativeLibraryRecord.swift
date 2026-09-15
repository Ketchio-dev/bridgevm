import Foundation

struct NativeLibraryIssue: Codable, Equatable {
    let code: String
    let path: String
    let message: String
}

struct NativeLibraryRecord: Codable, Equatable {
    let id: String
    let displayName: String
    let backendKind: String
    let cpuCount: Int?
    let memoryMiB: Int?
    let installPending: Bool?
    let configPath: String
    let bundlePath: String
    let runtimeState: String
    let recoveryObservations: [String]

    init(config: VMConfig, rootURL: URL) {
        id = config.slug
        displayName = config.displayName
        backendKind = config.backendKind
        cpuCount = config.cpuCount
        memoryMiB = config.memMiB
        installPending = config.installPending
        configPath = rootURL.appendingPathComponent(id).appendingPathComponent("vm.json").path
        bundlePath = config.bundlePath
        runtimeState = "unobserved"
        var observations: [String] = []
        if VMRelocationJournal.isPending(config, rootURL: rootURL) {
            observations.append("relocation-record-present-or-unreadable")
        }
        let journal = HvfWindowsInstallFinalizationPaths(
            libraryRoot: rootURL, bundle: URL(fileURLWithPath: config.bundlePath), slug: id).journal
        if NativeLibraryReader.entryMayExist(journal) {
            observations.append("install-finalization-record-present-or-unreadable")
        }
        recoveryObservations = observations
    }
}

struct NativeLibrarySnapshot: Encodable {
    let schema = "bridgevm.app-library.v1"
    let libraryPath: String
    let records: [NativeLibraryRecord]
    let issues: [NativeLibraryIssue]
    var complete: Bool { issues.isEmpty }

    enum CodingKeys: String, CodingKey { case schema, libraryPath, records, issues, complete }
    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(schema, forKey: .schema)
        try container.encode(libraryPath, forKey: .libraryPath)
        try container.encode(records, forKey: .records)
        try container.encode(issues, forKey: .issues)
        try container.encode(complete, forKey: .complete)
    }
}
