import Foundation

enum HvfWindowsSnapshotCommand {
    enum Operation { case create, restore, delete }

    static let errorDomain = "BridgeVM.HvfWindowsSnapshot"

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

    /// Creation timestamp from the filename, or manifest date for a legacy snapshot.
    static func snapshotDate(config: HvfEngineConfig) -> Date? {
        guard let bundle = HvfDiskGrow.bundle(config: config) else { return nil }
        return HvfWindowsSnapshotStore.list(bundle: bundle).first?.date
    }

    struct Plan {
        let executable: URL
        let disk: URL
        let vars: URL
        let snapshot: URL
        let vmID: String
        var name: String? = nil
        var bundle: URL { disk.deletingLastPathComponent().deletingLastPathComponent() }

        func arguments(for operation: Operation) throws -> [String] {
            switch operation {
            case .create:
                return try createArguments(destination: snapshot)
            case .restore:
                return ["restore", snapshot.path, disk.path, vars.path]
            case .delete:
                return []
            }
        }
    }

    static func plan(
        config: HvfEngineConfig,
        repoRoot: URL,
        operation: Operation,
        target: URL? = nil,
        name: String? = nil,
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
        let directory = HvfWindowsSnapshotStore.directory(bundle: bundle)
        let snapshot: URL
        if operation == .create {
            let occupied = Set((try? fileManager.contentsOfDirectory(atPath: directory.path)) ?? [])
            snapshot = HvfWindowsSnapshotStore.nextURL(bundle: bundle, occupied: occupied)
        } else {
            guard let selected = target ?? HvfWindowsSnapshotStore.list(bundle: bundle).first?.url else {
                throw failure("the verified powered-off snapshot is unavailable", .noSnapshot)
            }
            snapshot = try HvfWindowsSnapshotStore.validatedTarget(selected, bundle: bundle)
        }
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
            executable: executable, disk: disk, vars: vars, snapshot: snapshot, vmID: vmID, name: name)
    }

    /// A successful operation may carry a non-fatal display-name notice.
    static func run(_ operation: Operation, plan: Plan, checkAdmission: @escaping @Sendable () async throws -> Void = {}) async throws -> String? {
        try await Task.detached {
            try await checkAdmission()
            if operation == .delete {
                try HvfWindowsSnapshotStore.delete(plan.snapshot, bundle: plan.bundle)
                return nil
            }
            if operation == .create {
                try FileManager.default.createDirectory(
                    at: plan.snapshot.deletingLastPathComponent(), withIntermediateDirectories: true)
            }
            try await checkAdmission()
            let arguments = try plan.arguments(for: operation)
            try await checkAdmission()
            if operation == .create, FileManager.default.fileExists(atPath: plan.snapshot.path) {
                throw failure("snapshot destination was occupied before creation", .unsafeFolder)
            }
            _ = try invoke(plan.executable, arguments)
            if operation == .create {
                try await checkAdmission()
                _ = try invoke(plan.executable, ["verify", plan.snapshot.path])
                if let name = plan.name, !name.isEmpty {
                    do {
                        try name.write(to: HvfWindowsSnapshotStore.nameFile(plan.snapshot), atomically: true, encoding: .utf8)
                    } catch {
                        return "스냅샷은 생성했지만 이름을 저장하지 못해 날짜만 표시합니다."
                    }
                }
            }
            return nil
        }.value
    }

    static func invoke(_ executable: URL, _ arguments: [String]) throws -> String {
        let completion = try HvfHelperProcess.run(executable, arguments)
        guard completion.succeeded else { throw failure(completion.diagnostic) }
        return completion.text
    }

}
