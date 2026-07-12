import XCTest
@testable import BridgeVMControl

final class HvfEngineConfigTests: XCTestCase {
    func testBaseArgumentsUseExplicitServiceCLI() {
        let cfg = HvfEngineConfig(targetDiskPath: "/vm/win.raw",
                                  uefiVarsPath: "/vm/vars.fd",
                                  evidenceDir: "/tmp/evidence",
                                  watchdogMs: 123_000,
                                  ramMiB: 6144,
                                  smpCpus: 4,
                                  clipboardSync: false,
                                  shareHostDir: nil,
                                  shareGuestDir: nil,
                                  virtioNet: false,
                                  ctlFilePath: "/tmp/evidence/ctl")
        XCTAssertEqual(cfg.wrapperArguments(), [
            "scripts/run-hvf-windows-installed-boot.sh",
            "--target", "/vm/win.raw",
            "--vars", "/vm/vars.fd",
            "--evidence-dir", "/tmp/evidence",
            "--watchdog-ms", "123000",
            "--ram-mib", "6144",
            "--smp-cpus", "4",
            "--release",
            "--skip-build",
            "--agent-service-control", "/tmp/evidence/ctl",
            "--agent-service-command", "whoami"
        ])
    }

    func testArgumentsWithAllToggles() {
        let cfg = HvfEngineConfig(targetDiskPath: "/vm/win.raw",
                                  uefiVarsPath: "/vm/vars.fd",
                                  evidenceDir: "/tmp/evidence",
                                  watchdogMs: 480_000,
                                  ramMiB: 8192,
                                  smpCpus: 8,
                                  clipboardSync: true,
                                  shareHostDir: "/Users/me/share",
                                  shareGuestDir: "C:\\share",
                                  virtioNet: true,
                                  ctlFilePath: "/tmp/evidence/ctl")
        let args = cfg.wrapperArguments()
        XCTAssertTrue(args.contains("--virtio-net"))
        XCTAssertTrue(args.contains("--agent-clipboard-sync"))
        XCTAssertTrue(args.contains("--release"))
        XCTAssertTrue(args.contains("--skip-build"))
        XCTAssertEqual(value(after: "--ram-mib", in: args), "8192")
        XCTAssertEqual(value(after: "--smp-cpus", in: args), "8")
        XCTAssertEqual(value(after: "--agent-share-host", in: args), "/Users/me/share")
        XCTAssertEqual(value(after: "--agent-share-guest", in: args), "C:\\share")
        XCTAssertEqual(value(after: "--agent-share-ms", in: args), "2000")
        XCTAssertEqual(value(after: "--agent-share-max-kb", in: args), "65536")
    }

    func testPartialShareDoesNotEmitShareArguments() {
        let cfg = HvfEngineConfig(targetDiskPath: "t",
                                  uefiVarsPath: "v",
                                  evidenceDir: "e",
                                  watchdogMs: 1,
                                  ramMiB: 4096,
                                  smpCpus: 1,
                                  clipboardSync: false,
                                  shareHostDir: "/host",
                                  shareGuestDir: nil,
                                  virtioNet: false,
                                  ctlFilePath: "c")
        XCTAssertFalse(cfg.wrapperArguments().contains("--agent-share-host"))
        XCTAssertFalse(cfg.wrapperArguments().contains("--agent-share-ms"))
        XCTAssertFalse(cfg.wrapperArguments().contains("--agent-share-max-kb"))
    }

    func testDisabledWatchdogEmitsExplicitNoWatchdogPolicy() {
        let cfg = HvfEngineConfig(targetDiskPath: "t",
                                  uefiVarsPath: "v",
                                  evidenceDir: "e",
                                  watchdogMs: nil,
                                  ramMiB: 6144,
                                  smpCpus: 4,
                                  clipboardSync: false,
                                  shareHostDir: nil,
                                  shareGuestDir: nil,
                                  virtioNet: false,
                                  ctlFilePath: "c")
        let args = cfg.wrapperArguments()
        XCTAssertTrue(args.contains("--no-watchdog"))
        XCTAssertFalse(args.contains("--watchdog-ms"))
    }

    private func value(after flag: String, in args: [String]) -> String? {
        guard let index = args.firstIndex(of: flag), args.indices.contains(index + 1) else { return nil }
        return args[index + 1]
    }
}

final class HvfEngineSessionPathTests: XCTestCase {
    func testFindsRepositoryAncestorAndHonorsValidOverride() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let discoveredRoot = temp.appendingPathComponent("discovered", isDirectory: true)
        let overrideRoot = temp.appendingPathComponent("override", isDirectory: true)
        let nested = discoveredRoot.appendingPathComponent("apps/macos/.build/debug", isDirectory: true)
        try makeWrapper(at: discoveredRoot)
        try makeWrapper(at: overrideRoot)
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)

        let discovered = HvfEngineSession.defaultRepoRoot(
            currentDirectoryPath: nested.path,
            environment: [:],
            executablePath: nil,
            resourcePath: nil
        )
        XCTAssertEqual(discovered.path, discoveredRoot.resolvingSymlinksInPath().path)

        let overridden = HvfEngineSession.defaultRepoRoot(
            currentDirectoryPath: nested.path,
            environment: ["BRIDGEVM_REPO_ROOT": overrideRoot.path],
            executablePath: nil,
            resourcePath: nil
        )
        XCTAssertEqual(overridden.path, overrideRoot.resolvingSymlinksInPath().path)
    }

    private func makeWrapper(at root: URL) throws {
        let scripts = root.appendingPathComponent("scripts", isDirectory: true)
        try FileManager.default.createDirectory(at: scripts, withIntermediateDirectories: true)
        let wrapper = scripts.appendingPathComponent("run-hvf-windows-installed-boot.sh")
        try "#!/usr/bin/env bash\n".write(to: wrapper, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: wrapper.path)
    }
}

final class BvAgentEventTests: XCTestCase {
    func testParserExtractsReadyAndServiceTiming() {
        let events = BvAgentEvent.parse(lines: [
            "BVAGENT READY host=WIN11 v3-share2 t=42",
            "BVAGENT SERVICE start t=50",
            "BVAGENT SERVICE alive t=75",
            "BVAGENT SERVICE overdue ctl awaiting-reply=true t=90"
        ])
        XCTAssertEqual(events, [
            .ready(host: "WIN11 v3-share2", tMs: 42),
            .serviceStart(tMs: 50),
            .aliveHeartbeat(tMs: 75),
            .overdue(kind: "ctl", awaitingReply: true, tMs: 90)
        ])
    }

    func testParserMapsLegacyServiceTimeoutToNonfatalOverdueEvent() {
        XCTAssertEqual(
            BvAgentEvent.parse(lines: ["BVAGENT SERVICE timeout CLIPGET t=90"]),
            [.overdue(kind: "CLIPGET", awaitingReply: false, tMs: 90)]
        )
    }

    func testParserGroupsCommandOutput() {
        let events = BvAgentEvent.parse(lines: [
            "BVAGENT CMD whoami exit=0",
            "desktop-abc\\user",
            "second line",
            "BVAGENT END whoami"
        ])
        XCTAssertEqual(events, [.commandOutput(label: "whoami", body: "desktop-abc\\user\nsecond line")])
    }

    func testParserExtractsClipboardAndShareEvents() {
        let events = BvAgentEvent.parse(lines: [
            "BVAGENT CLIPSYNC host->guest bytes=12 t=101",
            "BVAGENT CLIPSYNC guest->host bytes=13 t=102",
            "BVAGENT SHARE host->guest notes.txt bytes=5 t=201",
            "BVAGENT SHARE guest->host report.txt bytes=7 t=202",
            "BVAGENT SHARE del host->guest old.txt t=203"
        ])
        XCTAssertEqual(events, [
            .clipSync(direction: .hostToGuest, bytes: 12, tMs: 101),
            .clipSync(direction: .guestToHost, bytes: 13, tMs: 102),
            .shareEvent(kind: .hostToGuest, path: "notes.txt", bytes: 5, tMs: 201),
            .shareEvent(kind: .guestToHost, path: "report.txt", bytes: 7, tMs: 202),
            .shareEvent(kind: .delete, path: "old.txt", bytes: nil, tMs: 203)
        ])
    }
}

final class PpmDecoderTests: XCTestCase {
    func testDecodesTwoByTwoP6Fixture() throws {
        var data = Data("P6\n2 2\n255\n".utf8)
        data.append(contentsOf: [
            255, 0, 0,
            0, 255, 0,
            0, 0, 255,
            255, 255, 255
        ])
        let decoded = try PpmDecoder.decode(data: data)
        XCTAssertEqual(decoded.width, 2)
        XCTAssertEqual(decoded.height, 2)
        XCTAssertEqual(Array(decoded.rgba), [
            255, 0, 0, 255,
            0, 255, 0, 255,
            0, 0, 255, 255,
            255, 255, 255, 255
        ])
    }
}

final class TailOffsetReaderTests: XCTestCase {
    func testReadsAppendsAndResetsAfterTruncation() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("run.log")
        try "one\n".write(to: file, atomically: true, encoding: .utf8)

        let reader = TailOffsetReader()
        XCTAssertEqual(reader.readNewLines(from: file), ["one"])

        let handle = try FileHandle(forWritingTo: file)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("two\npartial".utf8))
        try handle.close()
        XCTAssertEqual(reader.readNewLines(from: file), ["two"])

        let handle2 = try FileHandle(forWritingTo: file)
        try handle2.seekToEnd()
        try handle2.write(contentsOf: Data("-done\n".utf8))
        try handle2.close()
        XCTAssertEqual(reader.readNewLines(from: file), ["partial-done"])

        try "reset\n".write(to: file, atomically: true, encoding: .utf8)
        XCTAssertEqual(reader.readNewLines(from: file), ["reset"])
    }
}

final class HvfWindowsBackendTests: XCTestCase {
    func testPathKindAndDisplayNameDerivation() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg)

        XCTAssertEqual(backend.displayName, "Windows Preview")
        XCTAssertEqual(backend.kind, "hvf-engine")
        XCTAssertTrue(backend.supportsGuestCommands)
        XCTAssertFalse(backend.supportsPackageInstall)
        XCTAssertTrue(backend.supportsClipboard)
        XCTAssertFalse(backend.supportsSSH)
        XCTAssertEqual(backend.targetDiskPath, cfg.bundlePath + "/disks/hvf-target.raw")
        XCTAssertEqual(backend.uefiVarsPath, cfg.bundlePath + "/metadata/hvf-vars.fd")
        XCTAssertEqual(backend.evidenceDir, cfg.bundlePath + "/logs/hvf")
        XCTAssertEqual(backend.ctlFilePath, cfg.bundlePath + "/metadata/hvf.ctl")
    }

    func testLaunchCommandUsesExplicitCLIResourcesAndSeparateLauncherLog() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg)

        let command = backend.launchCommand()
        XCTAssertTrue(command.contains("'--agent-service-control' '\(backend.ctlFilePath)'"))
        XCTAssertTrue(command.contains("'--ram-mib' '4096'"))
        XCTAssertTrue(command.contains("'--smp-cpus' '1'"))
        XCTAssertTrue(command.contains(">\(Shell.shQuote(backend.launcherLogPath)) 2>&1"))
        XCTAssertFalse(command.contains(">\(Shell.shQuote(backend.evidenceDir + "/run.log")) 2>&1"))
        XCTAssertFalse(command.contains("BRIDGEVM_VIRTIO_CONSOLE="))
    }

    func testRunInGuestAppendsCtlAndParsesReply() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg)
        let log = URL(fileURLWithPath: backend.evidenceDir).appendingPathComponent("run.log")

        DispatchQueue.global().asyncAfter(deadline: .now() + 0.2) {
            try? FileManager.default.createDirectory(atPath: backend.evidenceDir, withIntermediateDirectories: true)
            try? """
            BVAGENT CMD foo exit=0
            hello
            world
            BVAGENT END foo

            """.write(to: log, atomically: true, encoding: .utf8)
        }

        let result = backend.runInGuest("foo")
        XCTAssertEqual(result.output, "hello\nworld")
        XCTAssertEqual(result.code, 0)
        XCTAssertEqual(try String(contentsOfFile: backend.ctlFilePath, encoding: .utf8), "foo\n")
    }

    func testGracefulStopRequestUsesAgentControlChannel() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg)

        backend.requestGracefulStop()

        XCTAssertEqual(
            try String(contentsOfFile: backend.ctlFilePath, encoding: .utf8),
            "shutdown.exe /p /f\n"
        )
    }

    func testConcurrentControlWritesRemainWholeAndLossless() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg)

        DispatchQueue.concurrentPerform(iterations: 32) { _ in
            backend.requestGracefulStop()
        }

        let lines = try String(contentsOfFile: backend.ctlFilePath, encoding: .utf8)
            .split(separator: "\n")
        XCTAssertEqual(lines.count, 32)
        XCTAssertTrue(lines.allSatisfy { $0 == "shutdown.exe /p /f" })
    }

    func testPendingDiskGrowthCommandIsIdempotentOnlyAtDiskEnd() {
        let command = HvfWindowsBackend.pendingDiskGrowthCommand

        XCTAssertFalse(command.contains("\n"))
        XCTAssertTrue(command.contains("$tailGap -gt 16777216"))
        XCTAssertTrue(command.contains("throw 'C: has no contiguous extension space'"))
        XCTAssertTrue(command.contains("$state='already-max'"))
        XCTAssertTrue(command.contains("BRIDGEVM_DISK_GROW_OK state="))
    }

    private func makeTempDir() throws -> URL {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private func makeConfig(bundlePath: String, diskPath: String? = nil) -> VMConfig {
        VMConfig(id: "win-hvf", name: "Win HVF", displayName: "Windows Preview", backendKind: "hvf-engine",
                 bootMode: "windows-hvf", bundlePath: bundlePath, runnerPath: "", launchSpecPath: "",
                 handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                 guestName: "win-hvf", displayWidth: 1280, displayHeight: 800,
                 diskPath: diskPath, memMiB: 4096, cpuCount: 1)
    }
}
