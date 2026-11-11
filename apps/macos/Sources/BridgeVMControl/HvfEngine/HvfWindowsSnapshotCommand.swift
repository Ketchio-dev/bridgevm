import Foundation

enum HvfWindowsSnapshotCommand {
    enum Operation { case create, restore }

    static let errorDomain = "BridgeVM.HvfWindowsSnapshot"
    /// The VM bundle's one snapshot; a create replaces it.
    static let slot = "metadata/snapshots/latest.snapshot"

    /// What a failure was, as its error code. The English description is for the CLI and the event
    /// feed; the card says each in Korean (HvfWindowsSnapshotModel.message).
    enum Failure: Int {
        /// The helper failed; its output is the description.
        case helper = 1
        case notStopped, settingsUnsaved, notLibraryVM, helperMissing, unsafeFolder, noSnapshot
    }

    static func notStopped() -> NSError {
        failure("the VM is running, or other work holds it", .notStopped)
    }

    /// When the snapshot was made: when its manifest, written last, was written; nil without one.
    static func snapshotDate(config: HvfEngineConfig) -> Date? {
        guard let bundle = HvfDiskGrow.bundle(config: config) else { return nil }
        let manifest = bundle.appendingPathComponent(slot).appendingPathComponent("manifest.json")
        return try? manifest.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate
    }

    struct Plan {
        let executable: URL
        let disk: URL
        let vars: URL
        let snapshot: URL
        let vmID: String

        func arguments(for operation: Operation) throws -> [String] {
            switch operation {
            case .create:
                return try createArguments(destination: snapshot)
            case .restore:
                return ["restore", snapshot.path, disk.path, vars.path]
            }
        }
    }

    static func plan(
        config: HvfEngineConfig,
        repoRoot: URL,
        operation: Operation,
        fileManager: FileManager = .default
    ) throws -> Plan {
        let disk = URL(fileURLWithPath: config.targetDiskPath).standardizedFileURL
        let bundle = disk.deletingLastPathComponent().deletingLastPathComponent()
        let vars = URL(fileURLWithPath: config.uefiVarsPath).standardizedFileURL
        let expectedDisk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let expectedVars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        guard disk.path == expectedDisk.path, vars.path == expectedVars.path,
              canonical(disk), canonical(vars), regularFile(disk), regularFile(vars) else {
            throw failure("snapshot media is outside the managed Windows HVF bundle", .notLibraryVM)
        }
        guard let vmID = config.vtpmKeyID, vmID == VMConfig.slugify(vmID) else {
            throw failure("snapshot requires the VM stable identifier", .notLibraryVM)
        }
        let executable = repoRoot.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        guard canonical(executable), regularFile(executable),
              fileManager.isExecutableFile(atPath: executable.path) else {
            throw failure("the bundled snapshot helper is missing or unsafe", .helperMissing)
        }
        let snapshot = bundle.appendingPathComponent(slot)
        let parent = snapshot.deletingLastPathComponent()
        guard canonical(bundle), canonical(snapshot.deletingLastPathComponent().deletingLastPathComponent()) else {
            throw failure("snapshot destination crosses a symbolic link", .unsafeFolder)
        }
        if operation == .restore {
            guard regularDirectory(snapshot), canonical(snapshot) else {
                throw failure("the verified powered-off snapshot is unavailable", .noSnapshot)
            }
        } else if fileManager.fileExists(atPath: parent.path) {
            guard regularDirectory(parent), canonical(parent) else {
                throw failure("snapshot directory is unsafe", .unsafeFolder)
            }
        }
        return Plan(
            executable: executable, disk: disk, vars: vars, snapshot: snapshot, vmID: vmID)
    }

    static func run(_ operation: Operation, plan: Plan, checkAdmission: @escaping @Sendable () async throws -> Void = {}) async throws -> String {
        try await Task.detached {
            try await checkAdmission()
            if operation == .create {
                try FileManager.default.createDirectory(
                    at: plan.snapshot.deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            try await checkAdmission()
            let arguments = try plan.arguments(for: operation)
            try await checkAdmission()
            let output = try invoke(plan.executable, arguments)
            if operation == .create {
                try await checkAdmission()
                _ = try invoke(plan.executable, ["verify", plan.snapshot.path])
            }
            return output
        }.value
    }

    static func invoke(_ executable: URL, _ arguments: [String]) throws -> String {
        let completion = try HvfHelperProcess.run(executable, arguments)
        guard completion.succeeded else { throw failure(completion.diagnostic) }
        return completion.text
    }

}
