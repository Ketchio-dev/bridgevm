import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLISnapshotTests: XCTestCase {
    func testOptionsAcceptOnlyExactSnapshotCommandAndCanonicalID() throws {
        XCTAssertEqual(
            try NativeCLIOptions.parse(arguments: ["snapshot-create", "개발-vm"]).command,
            .snapshotCreate("개발-vm")
        )
        XCTAssertEqual(
            try NativeCLIOptions.parse(arguments: ["snapshot-restore", "개발-vm"]).command,
            .snapshotRestore("개발-vm")
        )
        for verb in ["snapshot-create", "snapshot-restore"] {
            XCTAssertTrue(try NativeCLIOptions.parse(arguments: [verb, "--help"]).showHelp)
            for arguments in [
                [verb],
                [verb, "../vm"],
                [verb, "vm", "extra"],
                [verb, "vm", "--disk", "/secret.raw"],
            ] {
                XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: arguments))
            }
        }
        XCTAssertTrue(NativeCLI.help.contains("bridgevm.app-snapshot.v1"))
    }

    func testCreateVerifiesAndRestoreUseOnlySavedManagedPaths() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }

        let created = NativeCLISnapshot.run(
            rootURL: fixture.library,
            id: fixture.id,
            operation: .create,
            repoRoot: fixture.repo
        )
        XCTAssertTrue(created.complete, created.unavailableReason ?? "")
        XCTAssertEqual(created.schema, "bridgevm.app-snapshot.v1")
        XCTAssertEqual(created.snapshotPath, fixture.snapshot.path)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fixture.snapshot.path))
        XCTAssertEqual(
            try fixture.invocations(),
            [
                ["create", fixture.disk.path, fixture.vars.path, fixture.snapshot.path, fixture.id, "68"],
                ["verify", fixture.snapshot.path],
            ]
        )

        let restored = NativeCLISnapshot.run(
            rootURL: fixture.library,
            id: fixture.id,
            operation: .restore,
            repoRoot: fixture.repo
        )
        XCTAssertTrue(restored.complete, restored.unavailableReason ?? "")
        XCTAssertEqual(
            try fixture.invocations().last,
            ["restore", fixture.snapshot.path, fixture.disk.path, fixture.vars.path]
        )
        XCTAssertTrue(restored.text.contains("does not prove a Windows boot"))
    }

    func testPendingAndUnsupportedVMsRefuseBeforeHelperDispatch() throws {
        for (backend, pending) in [("hvf-engine", true), ("fast-vz", false)] {
            let fixture = try Fixture(backend: backend, pending: pending)
            addTeardownBlock { fixture.remove() }
            let result = NativeCLISnapshot.run(
                rootURL: fixture.library,
                id: fixture.id,
                operation: .create,
                repoRoot: fixture.repo
            )
            XCTAssertFalse(result.complete)
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.log.path))
            XCTAssertNil(result.snapshotPath)
        }
    }

    private struct Fixture {
        let root: URL
        let library: URL
        let repo: URL
        let id = "개발-vm"
        let disk: URL
        let vars: URL
        let snapshot: URL
        let log: URL

        init(backend: String = "hvf-engine", pending: Bool = false) throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("native-cli-snapshot-" + UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            repo = root.appendingPathComponent("repo", isDirectory: true)
            let entry = library.appendingPathComponent(id, isDirectory: true)
            let bundle = entry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
            disk = bundle.appendingPathComponent("disks/hvf-target.raw")
            vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
            snapshot = bundle.appendingPathComponent("metadata/snapshots/latest.snapshot")
            log = root.appendingPathComponent("helper-invocations.jsonl")
            let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            try FileManager.default.createDirectory(
                at: disk.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try FileManager.default.createDirectory(
                at: vars.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try FileManager.default.createDirectory(
                at: helper.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try Data(count: 64).write(to: disk)
            try Data(count: 4).write(to: vars)
            let script = """
            #!/bin/sh
            printf '%s\\n' "$@" | "\(try NativeTestPython.executable().path)" -c 'import json,sys; print(json.dumps(sys.stdin.read().splitlines()))' >> "\(log.path)"
            if [ "$1" = create ]; then mkdir -p "$4"; fi
            """
            try Data(script.utf8).write(to: helper)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o755],
                ofItemAtPath: helper.path
            )
            let config = VMConfig(
                id: id,
                name: "개발 VM",
                displayName: "개발 VM",
                backendKind: backend,
                bootMode: "windows-hvf",
                bundlePath: bundle.path,
                runnerPath: "",
                launchSpecPath: "",
                handoffPath: "",
                sshKeyPath: "",
                sshUser: "",
                leasesPath: "",
                guestName: "Windows",
                displayWidth: 1280,
                displayHeight: 800,
                installPending: pending,
                memMiB: 6144,
                cpuCount: 4,
                experimental3DAllowed: false
            )
            try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
        }

        func invocations() throws -> [[String]] {
            try String(contentsOf: log, encoding: .utf8)
                .split(separator: "\n")
                .map { try JSONDecoder().decode([String].self, from: Data($0.utf8)) }
        }

        func remove() {
            try? FileManager.default.removeItem(at: root)
        }
    }
}
