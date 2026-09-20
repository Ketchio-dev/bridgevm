import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLISnapshotExportTests: XCTestCase {
    func testParserAcceptsExactIDAndAbsoluteOutputWithFlagsInAnyOrder() throws {
        let library = URL(fileURLWithPath: "/private/tmp/export-library", isDirectory: true)
        let output = URL(fileURLWithPath: "/private/tmp/current.snapshot", isDirectory: true)
        for arguments in [
            ["snapshot-export", "개발-vm", output.path, "--json", "--library", library.path],
            ["--library", library.path, "snapshot-export", "개발-vm", output.path, "--json"],
        ] {
            let parsed = try NativeCLIOptions.parse(arguments: arguments)
            guard case .snapshotExport(let options) = parsed.command else {
                return XCTFail("expected snapshot-export")
            }
            XCTAssertEqual(options, .init(id: "개발-vm", output: output))
            XCTAssertEqual(parsed.libraryRoot, library)
            XCTAssertTrue(parsed.json)
        }
        XCTAssertTrue(try NativeCLIOptions.parse(arguments: ["snapshot-export", "--help"]).showHelp)
        XCTAssertTrue(NativeCLI.help.contains("snapshot-export ID OUTPUT"))
    }

    func testParserRejectsAmbiguousOrUnsafeArguments() {
        for arguments in [
            ["snapshot-export"],
            ["snapshot-export", "../vm", "/tmp/export"],
            ["snapshot-export", "vm", "relative"],
            ["snapshot-export", "vm", "/tmp/../export"],
            ["snapshot-export", "vm", "/"],
            ["snapshot-export", "vm", "/tmp/export", "extra"],
            ["snapshot-export", "vm", "/tmp/export", "--json", "--json"],
        ] {
            XCTAssertThrowsError(try NativeCLIOptions.parse(arguments: arguments), "accepted \(arguments)")
        }
    }

    func testExportCreatesThenVerifiesCallerDestination() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }
        let result = NativeCLISnapshotExport.run(
            rootURL: fixture.library,
            options: .init(id: fixture.id, output: fixture.output),
            repoRoot: fixture.repo
        )
        XCTAssertTrue(result.complete, result.unavailableReason ?? "")
        XCTAssertEqual(result.schema, "bridgevm.app-snapshot.v1")
        XCTAssertEqual(result.command, "export")
        XCTAssertEqual(result.snapshotPath, fixture.output.path)
        let encoded = try JSONSerialization.jsonObject(with: JSONEncoder().encode(result)) as? [String: Any]
        XCTAssertEqual(encoded?["schema"] as? String, "bridgevm.app-snapshot.v1")
        XCTAssertEqual(encoded?["command"] as? String, "export")
        XCTAssertEqual(encoded?["vmID"] as? String, fixture.id)
        XCTAssertEqual(encoded?["snapshotPath"] as? String, fixture.output.path)
        XCTAssertEqual(encoded?["complete"] as? Bool, true)
        XCTAssertNil(encoded?["unavailableReason"])
        XCTAssertEqual(
            try fixture.invocations(),
            [
                ["create", fixture.disk.path, fixture.vars.path, fixture.output.path, fixture.id, "68"],
                ["verify", fixture.output.path],
            ]
        )
        XCTAssertTrue(result.text.contains("does not prove a Windows boot"))
    }

    func testExportInsideManagedBundleRefusesBeforeDispatch() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }
        let result = NativeCLISnapshotExport.run(
            rootURL: fixture.library,
            options: .init(id: fixture.id, output: fixture.bundle.appendingPathComponent("unsafe.snapshot")),
            repoRoot: fixture.repo
        )
        XCTAssertFalse(result.complete)
        XCTAssertNil(result.snapshotPath)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.log.path))
    }

    func testExportParentSymlinkIntoManagedBundleRefusesBeforeDispatch() throws {
        let fixture = try Fixture()
        addTeardownBlock { fixture.remove() }
        let alias = fixture.root.appendingPathComponent("bundle-alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: fixture.bundle)
        let result = NativeCLISnapshotExport.run(
            rootURL: fixture.library,
            options: .init(id: fixture.id, output: alias.appendingPathComponent("unsafe.snapshot")),
            repoRoot: fixture.repo
        )
        XCTAssertFalse(result.complete)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.log.path))
    }

    private final class Fixture {
        let root: URL
        let library: URL
        let repo: URL
        let id = "개발-vm"
        let bundle: URL
        let disk: URL
        let vars: URL
        let output: URL
        let log: URL

        init() throws {
            root = FileManager.default.temporaryDirectory
                .appendingPathComponent("native-cli-export-" + UUID().uuidString)
            library = root.appendingPathComponent("library", isDirectory: true)
            repo = root.appendingPathComponent("repo", isDirectory: true)
            let entry = library.appendingPathComponent(id, isDirectory: true)
            bundle = entry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
            disk = bundle.appendingPathComponent("disks/hvf-target.raw")
            vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
            output = root.appendingPathComponent("exports/current.snapshot", isDirectory: true)
            log = root.appendingPathComponent("helper-invocations.jsonl")
            let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
            for directory in [disk.deletingLastPathComponent(), vars.deletingLastPathComponent(), helper.deletingLastPathComponent()] {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            }
            try Data(count: 64).write(to: disk)
            try Data(count: 4).write(to: vars)
            let script = """
            #!/bin/sh
            printf '%s\\n' "$@" | "\(try NativeTestPython.executable().path)" -c 'import json,sys; print(json.dumps(sys.stdin.read().splitlines()))' >> "\(log.path)"
            if [ "$1" = create ]; then mkdir -p "$4"; fi
            """
            try Data(script.utf8).write(to: helper)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: helper.path)
            let config = VMConfig(
                id: id, name: "개발 VM", displayName: "개발 VM", backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
                handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
                displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
                cpuCount: 4, experimental3DAllowed: false
            )
            try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
        }

        func invocations() throws -> [[String]] {
            try String(contentsOf: log, encoding: .utf8).split(separator: "\n")
                .map { try JSONDecoder().decode([String].self, from: Data($0.utf8)) }
        }

        func remove() { try? FileManager.default.removeItem(at: root) }
    }
}
