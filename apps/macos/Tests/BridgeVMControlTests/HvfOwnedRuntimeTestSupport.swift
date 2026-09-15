import Foundation
import Darwin
import XCTest
@testable import BridgeVMControl

final class HvfOwnedRuntimeKeys: VTPMStateKeyProviding {
    var requests = 0
    func stateKey(for stableVMID: String, allowCreation: Bool) throws -> Data {
        requests += 1
        XCTAssertTrue(allowCreation)
        return Data(repeating: 0x42, count: 32) // Synthetic transport bytes; no vTPM state is opened.
    }
}

@MainActor
final class HvfOwnedRuntimeFixture {
    let root: URL
    let keys = HvfOwnedRuntimeKeys()
    var child: Process?
    var launches = 0, lookups = 0
    var existingRuntime = false
    var failLaunch = false, failKeyDelivery = false
    var script = "exit 17"
    var config: HvfEngineConfig
    var ready: URL { root.appendingPathComponent("child-ready") }
    var release: URL { root.appendingPathComponent("child-release") }

    init(readyForLaunch: Bool = true, encrypted: Bool = false) throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("owned-runtime-" + UUID().uuidString)
        self.root = root
        config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("synthetic.raw").path,
            uefiVarsPath: root.appendingPathComponent("synthetic-vars.fd").path,
            evidenceDir: root.appendingPathComponent("evidence").path,
            watchdogMs: nil, ramMiB: 1024, smpCpus: 1, clipboardSync: false,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: false, audioEnabled: false,
            virtioGpu3d: false, nvmeBufferedIO: false, ctlFilePath: root.appendingPathComponent("control").path)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        if readyForLaunch {
            try Data([0]).write(to: URL(fileURLWithPath: config.targetDiskPath))
            FileManager.default.createFile(atPath: config.uefiVarsPath, contents: nil)
            let vars = try FileHandle(forWritingTo: URL(fileURLWithPath: config.uefiVarsPath))
            try vars.truncate(atOffset: 64 * 1024 * 1024)
            try vars.close()
            for name in ["scripts/run-hvf-windows-installed-boot.sh", "target/release/examples/hvf_gic_boot_probe"] {
                let url = root.appendingPathComponent(name)
                try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
                try Data("owned readiness marker; never executed\n".utf8).write(to: url)
                try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: url.path)
            }
        }
        if encrypted {
            config.vtpmStateDir = root.appendingPathComponent("absent-synthetic-vtpm").path
            config.vtpmKeyID = "owned-fixture"
            config.swtpmBin = root.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path
        }
    }

    func waitScript(exit: Int = 0, ignoringTerm: Bool = false) {
        script = (ignoringTerm ? "trap '' TERM; " : "") + "printf ready > \"$1\"; "
            + "n=0; while [ ! -e \"$2\" ] && [ $n -lt 400 ]; do /bin/sleep 0.01; n=$((n+1)); done; "
            + "[ -e \"$2\" ] || exit 98; exit \(exit)"
    }

    func session() -> HvfEngineSession {
        HvfEngineSession(config: config, repoRoot: root, processLaunch: { [self] process in
            launches += 1
            if failLaunch { throw CocoaError(.executableNotLoadable) }
            process.executableURL = URL(fileURLWithPath: "/bin/sh")
            process.arguments = ["-c", script, "owned-runtime-child", ready.path, release.path]
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            let keyPipe = failKeyDelivery ? try XCTUnwrap(process.standardInput as? Pipe) : nil
            child = process
            try process.run()
            if failKeyDelivery {
                let deadline = ProcessInfo.processInfo.systemUptime + 2
                while !FileManager.default.fileExists(atPath: ready.path), process.isRunning,
                      ProcessInfo.processInfo.systemUptime < deadline { Thread.sleep(forTimeInterval: 0.005) }
                XCTAssertTrue(FileManager.default.fileExists(atPath: ready.path), "Child must acknowledge TERM handler")
                do { try keyPipe?.fileHandleForWriting.close() }
                catch { XCTFail("Unable to close the owned key transport: \(error)") }
            }
        }, processIsRunning: { [self] _ in lookups += 1; return existingRuntime }, vtpmKeyProvider: keys)
    }

    func releaseChild() throws { try Data().write(to: release) }

    func observeExit(_ session: HvfEngineSession) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 3
        while child?.isRunning == true, ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertFalse(child?.isRunning == true, "Only an observed child exit permits terminal assertions")
        session.poll()
    }

    func clean() {
        try? releaseChild()
        let deadline = ProcessInfo.processInfo.systemUptime + 0.5
        while child?.isRunning == true, ProcessInfo.processInfo.systemUptime < deadline {
            Thread.sleep(forTimeInterval: 0.005)
        }
        if let child, child.isRunning {
            // This exact retained test Process was created above; no path or process-name lookup.
            _ = Darwin.kill(child.processIdentifier, SIGKILL)
            let killedDeadline = ProcessInfo.processInfo.systemUptime + 1
            while child.isRunning, ProcessInfo.processInfo.systemUptime < killedDeadline {
                Thread.sleep(forTimeInterval: 0.005)
            }
        }
        XCTAssertFalse(child?.isRunning == true)
        if child?.isRunning != true { try? FileManager.default.removeItem(at: root) }
    }
}
