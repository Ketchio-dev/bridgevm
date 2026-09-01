import Foundation

enum HvfWindowsSnapshotCommand {
    enum Operation { case create, restore }

    struct Plan {
        let executable: URL
        let disk: URL
        let vars: URL
        let snapshot: URL
        let vmID: String
        let quotaBytes: UInt64

        func arguments(for operation: Operation) -> [String] {
            switch operation {
            case .create:
                return ["create", disk.path, vars.path, snapshot.path, vmID, String(quotaBytes)]
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
        let diskBytes = try fileSize(disk)
        let varsBytes = try fileSize(vars)
        let sum = diskBytes.addingReportingOverflow(varsBytes)
        guard !sum.overflow, sum.partialValue > 0 else {
            throw failure("snapshot quota cannot be calculated")
        }
        return Plan(
            executable: executable, disk: disk, vars: vars, snapshot: snapshot,
            vmID: vmID, quotaBytes: sum.partialValue)
    }

    static func run(_ operation: Operation, plan: Plan) async throws -> String {
        try await Task.detached {
            if operation == .create {
                try FileManager.default.createDirectory(
                    at: plan.snapshot.deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            let output = try invoke(plan.executable, plan.arguments(for: operation))
            if operation == .create {
                _ = try invoke(plan.executable, ["verify", plan.snapshot.path])
            }
            return output
        }.value
    }

    private static func invoke(_ executable: URL, _ arguments: [String]) throws -> String {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = executable
        process.arguments = arguments
        process.environment = ["PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "LANG": "C"]
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        process.waitUntilExit()
        let output = String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        guard process.terminationReason == .exit, process.terminationStatus == 0 else {
            throw failure(output.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return output
    }

    private static func regularFile(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else { return false }
        return values.isRegularFile == true && values.isSymbolicLink != true
    }

    private static func regularDirectory(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]) else { return false }
        return values.isDirectory == true && values.isSymbolicLink != true
    }

    private static func canonical(_ url: URL) -> Bool {
        url.standardizedFileURL.path == url.resolvingSymlinksInPath().standardizedFileURL.path
    }

    private static func fileSize(_ url: URL) throws -> UInt64 {
        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        guard let size = attributes[.size] as? NSNumber else { throw failure("media size is unavailable") }
        return size.uint64Value
    }

    private static func failure(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.HvfWindowsSnapshot", code: 1,
                userInfo: [NSLocalizedDescriptionKey: message.isEmpty ? "snapshot operation failed" : message])
    }
}
