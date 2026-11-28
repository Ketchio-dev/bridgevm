import Foundation
import Darwin

enum HvfWindowsSnapshotCommand {
    enum Operation { case create, restore, delete }

    static let errorDomain = "BridgeVM.HvfWindowsSnapshot"

    /// What a failure was, as its error code. The English description is for the CLI and the event
    /// feed; the card shows its own sentence for each (HvfWindowsSnapshotModel.message).
    enum Failure: Int {
        /// The helper failed; its output is the description.
        case helper = 1
        case notStopped, settingsUnsaved, notLibraryVM, helperMissing, unsafeFolder, noSnapshot
        /// The chosen snapshot was deleted since the list was shown (by the CLI, or in Finder).
        case snapshotGone
    }

    static func notStopped() -> NSError {
        failure("the VM is running, or other work holds it", .notStopped)
    }

    struct Plan {
        let executable: URL
        let disk: URL
        let vars: URL
        var snapshot: URL
        let vmID: String
        var name: String? = nil
        var bundle: URL { disk.deletingLastPathComponent().deletingLastPathComponent() }

        func createArguments(destination: URL) -> [String] {
            ["create", disk.path, vars.path, destination.path, vmID]
        }

        func arguments(for operation: Operation) throws -> [String] {
            switch operation {
            case .create:
                return createArguments(destination: snapshot)
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
            let arguments = try plan.arguments(for: operation)
            try await checkAdmission()
            if operation == .create {
                let created = try create(plan: plan, arguments: arguments)
                try await checkAdmission()
                return nameCreated(created)
            }
            try restore(plan: plan, arguments: arguments)
            return nil
        }.value
    }

    /// Shared by the session and native CLI. The restored Windows may run older Guest Tools than the
    /// app's, and this app session already tried (or skipped) their update, which the probe tries once
    /// per session: forgetting that attempt updates them at the next start.
    static func restore(plan: Plan, arguments: [String]) throws {
        _ = try invoke(plan.executable, arguments)
        try? FileManager.default.removeItem(at: plan.bundle.appendingPathComponent("metadata/agent-update.json"))
    }

    /// Shared by the session and native CLI: mkdir claims an empty destination exclusively.
    /// The helper accepts it and publishes with rename; a competing creator must choose a sibling.
    static func create(plan: Plan, arguments: [String]) throws -> Plan {
        let parent = plan.snapshot.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: true)
        guard canonical(parent), regularDirectory(parent) else {
            throw failure("snapshot directory is unsafe", .unsafeFolder)
        }
        var claimed = plan
        while mkdir(claimed.snapshot.path, 0o700) != 0 {
            guard errno == EEXIST else { throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO) }
            var occupied = Set(try FileManager.default.contentsOfDirectory(atPath: parent.path))
            occupied.insert(claimed.snapshot.lastPathComponent)
            claimed.snapshot = HvfWindowsSnapshotStore.nextURL(bundle: plan.bundle,
                now: HvfWindowsSnapshotStore.timestamp(plan.snapshot.lastPathComponent) ?? Date(), occupied: occupied)
        }
        // Remove only an empty failed reservation, never a published snapshot or unexpected data.
        defer { _ = rmdir(claimed.snapshot.path) }
        var arguments = arguments
        arguments[3] = claimed.snapshot.path
        _ = try invoke(claimed.executable, arguments)
        return claimed
    }

    /// Create already hashed both members into the manifest; restore verifies them before use.
    static func nameCreated(_ plan: Plan) -> String? {
        if let name = plan.name, !name.isEmpty {
            do {
                try name.write(to: HvfWindowsSnapshotStore.nameFile(plan.snapshot), atomically: true, encoding: .utf8)
            } catch {
                return "The snapshot was created and will appear under its date because its name couldn’t be saved."
            }
        }
        return nil
    }

    static func invoke(_ executable: URL, _ arguments: [String]) throws -> String {
        let completion = try HvfHelperProcess.run(executable, arguments)
        guard completion.succeeded else { throw failure(completion.diagnostic) }
        return completion.text
    }

}

extension HvfWindowsSnapshotCommand {
    static func regularFile(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]) else { return false }
        return values.isRegularFile == true && values.isSymbolicLink != true
    }

    static func regularDirectory(_ url: URL) -> Bool {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey]) else { return false }
        return values.isDirectory == true && values.isSymbolicLink != true
    }

    static func canonical(_ url: URL) -> Bool {
        url.standardizedFileURL.path == url.resolvingSymlinksInPath().standardizedFileURL.path
    }

    static func failure(_ message: String, _ code: Failure = .helper) -> NSError {
        NSError(domain: errorDomain, code: code.rawValue,
                userInfo: [NSLocalizedDescriptionKey: message.isEmpty ? "snapshot operation failed" : message])
    }
}
