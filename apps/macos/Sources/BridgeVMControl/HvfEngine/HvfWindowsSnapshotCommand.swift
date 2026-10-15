import Foundation

enum HvfWindowsSnapshotCommand {
    enum Operation { case create, restore }

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
            throw failure("snapshot media is outside the managed Windows HVF bundle")
        }
        guard let vmID = config.vtpmKeyID, vmID == VMConfig.slugify(vmID) else {
            throw failure("snapshot requires the VM stable identifier")
        }
        let executable = repoRoot.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        guard canonical(executable), regularFile(executable),
              fileManager.isExecutableFile(atPath: executable.path) else {
            throw failure("the bundled snapshot helper is missing or unsafe")
        }
        let snapshot = bundle.appendingPathComponent("metadata/snapshots/latest.snapshot")
        let parent = snapshot.deletingLastPathComponent()
        guard canonical(bundle), canonical(snapshot.deletingLastPathComponent().deletingLastPathComponent()) else {
            throw failure("snapshot destination crosses a symbolic link")
        }
        if operation == .restore {
            guard regularDirectory(snapshot), canonical(snapshot) else {
                throw failure("the verified powered-off snapshot is unavailable")
            }
        } else if fileManager.fileExists(atPath: parent.path) {
            guard regularDirectory(parent), canonical(parent) else {
                throw failure("snapshot directory is unsafe")
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
