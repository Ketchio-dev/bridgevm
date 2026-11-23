import Darwin
import Foundation
import XCTest
@testable import BridgeVMControl

/// Foundation starts the snapshot helper in its own process group, so Ctrl-C and SIGTERM reach
/// only the CLI. The real CLI must stop its helper, wait for it, and say what the VM holds.
final class NativeCLISnapshotCancelTests: XCTestCase {
    func testSignalStopsTheRunningRestoreHelperAndReportsTheOutcome() throws {
        let cli = try XCTUnwrap(ProcessInfo.processInfo.environment["BRIDGEVM_TEST_NATIVE_CLI"])
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("native-cli-snapshot-cancel-" + UUID().uuidString)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let library = root.appendingPathComponent("library"), repo = root.appendingPathComponent("repo")
        let pidFile = root.appendingPathComponent("helper.pid")
        try makeFixture(library: library, repo: repo, pidFile: pidFile)

        for (number, status) in [(SIGINT, Int32(130)), (SIGTERM, Int32(143))] {
            try? FileManager.default.removeItem(at: pidFile)
            let output = root.appendingPathComponent("cli-\(number).json")
            XCTAssertTrue(FileManager.default.createFile(atPath: output.path, contents: nil))
            let handle = try FileHandle(forWritingTo: output)
            defer { try? handle.close() }
            let process = Process()
            process.executableURL = URL(fileURLWithPath: cli)
            process.arguments = ["--cli", "snapshot-restore", "cancel-vm", "--library", library.path, "--json"]
            var environment = ProcessInfo.processInfo.environment
            environment["BRIDGEVM_REPO_ROOT"] = repo.path
            process.environment = environment
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = handle
            process.standardError = handle
            try process.run()
            addTeardownBlock { if process.isRunning { process.terminate() } }

            let helper = try XCTUnwrap(waitForPID(pidFile), "the fake helper never started")
            addTeardownBlock { if kill(helper, 0) == 0 { kill(helper, SIGKILL) } }
            XCTAssertEqual(kill(helper, 0), 0, "the helper is running before the signal")
            XCTAssertEqual(kill(process.processIdentifier, number), 0)
            XCTAssertTrue(waitUntil { !process.isRunning }, "the CLI did not exit after signal \(number)")
            XCTAssertTrue(waitUntil { kill(helper, 0) != 0 }, "the helper outlived the CLI after signal \(number)")

            XCTAssertEqual(process.terminationReason, .exit)
            XCTAssertEqual(process.terminationStatus, status)
            let json = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: output)) as? [String: Any])
            XCTAssertEqual(json["complete"] as? Bool, false)
            XCTAssertEqual(json["cancelSignal"] as? Int, Int(number))
            XCTAssertEqual(json["unavailableReason"] as? String,
                           "Cancelled; the VM's disk and UEFI vars are either unchanged or fully restored from the snapshot.")
        }
    }

    private func makeFixture(library: URL, repo: URL, pidFile: URL) throws {
        let fileManager = FileManager.default
        let entry = library.appendingPathComponent("cancel-vm", isDirectory: true)
        let bundle = entry.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let snapshot = HvfWindowsSnapshotStore.directory(bundle: bundle).appendingPathComponent("20260101-000000.snapshot")
        let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        // A debug CLI accepts BRIDGEVM_REPO_ROOT only when it holds the boot wrapper.
        let wrapper = repo.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        for directory in [bundle.appendingPathComponent("disks"), bundle.appendingPathComponent("metadata"), snapshot,
                          helper.deletingLastPathComponent(), wrapper.deletingLastPathComponent()] {
            try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try Data(count: 64).write(to: bundle.appendingPathComponent("disks/hvf-target.raw"))
        try Data(count: 4).write(to: bundle.appendingPathComponent("metadata/hvf-vars.fd"))
        try Data("{}".utf8).write(to: snapshot.appendingPathComponent("manifest.json"))
        // Like the real helper, one process for the whole operation; it outlives an unforwarded signal.
        try Data("#!/bin/sh\nprintf '%s\\n' $$ > \"\(pidFile.path)\"\nexec /bin/sleep 30\n".utf8).write(to: helper)
        try Data("#!/bin/sh\n".utf8).write(to: wrapper)
        for executable in [helper, wrapper] {
            try fileManager.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        }
        let config = VMConfig(
            id: "cancel-vm", name: "cancel-vm", displayName: "Cancel VM", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
            displayWidth: 1280, displayHeight: 800, installPending: false)
        try JSONEncoder().encode(config).write(to: entry.appendingPathComponent("vm.json"))
    }

    private func waitForPID(_ file: URL) -> pid_t? {
        var pid: pid_t?
        _ = waitUntil {
            pid = (try? String(contentsOf: file, encoding: .utf8))
                .flatMap { pid_t($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
            return pid != nil
        }
        return pid
    }

    private func waitUntil(seconds: TimeInterval = 15, _ condition: () -> Bool) -> Bool {
        let deadline = ProcessInfo.processInfo.systemUptime + seconds
        while !condition() {
            guard ProcessInfo.processInfo.systemUptime < deadline else { return false }
            Thread.sleep(forTimeInterval: 0.02)
        }
        return true
    }
}
