import XCTest
@testable import BridgeVMControl

final class HvfEngineConfigTests: XCTestCase {
    func testArgumentsAndBaseEnvironment() {
        let cfg = HvfEngineConfig(targetDiskPath: "/vm/win.raw",
                                  uefiVarsPath: "/vm/vars.fd",
                                  evidenceDir: "/tmp/evidence",
                                  watchdogMs: 123_000,
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
            "--watchdog-ms", "123000"
        ])
        XCTAssertEqual(cfg.environment(), [
            "BRIDGEVM_VIRTIO_CONSOLE": "1",
            "BRIDGEVM_VIRTIO_CONSOLE_TEST": "1",
            "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS": "480000",
            "BRIDGEVM_VIRTIO_CONSOLE_CMDS": "whoami",
            "BRIDGEVM_VIRTIO_CONSOLE_SERVICE": "1",
            "BRIDGEVM_VIRTIO_CONSOLE_CTL": "/tmp/evidence/ctl"
        ])
    }

    func testArgumentsAndEnvironmentWithAllToggles() {
        let cfg = HvfEngineConfig(targetDiskPath: "/vm/win.raw",
                                  uefiVarsPath: "/vm/vars.fd",
                                  evidenceDir: "/tmp/evidence",
                                  watchdogMs: 480_000,
                                  clipboardSync: true,
                                  shareHostDir: "/Users/me/share",
                                  shareGuestDir: "C:\\share",
                                  virtioNet: true,
                                  ctlFilePath: "/tmp/evidence/ctl")
        XCTAssertTrue(cfg.wrapperArguments().contains("--virtio-net"))
        let env = cfg.environment()
        XCTAssertEqual(env["BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC"], "1")
        XCTAssertEqual(env["BRIDGEVM_VIRTIO_CONSOLE_SHARE"], "/Users/me/share::C:\\share")
        XCTAssertEqual(env["BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS"], "2000")
    }

    func testPartialShareDoesNotEmitShareEnvironment() {
        let cfg = HvfEngineConfig(targetDiskPath: "t",
                                  uefiVarsPath: "v",
                                  evidenceDir: "e",
                                  watchdogMs: 1,
                                  clipboardSync: false,
                                  shareHostDir: "/host",
                                  shareGuestDir: nil,
                                  virtioNet: false,
                                  ctlFilePath: "c")
        XCTAssertNil(cfg.environment()["BRIDGEVM_VIRTIO_CONSOLE_SHARE"])
        XCTAssertNil(cfg.environment()["BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS"])
    }
}

final class BvAgentEventTests: XCTestCase {
    func testParserExtractsReadyAndServiceTiming() {
        let events = BvAgentEvent.parse(lines: [
            "BVAGENT READY host=WIN11 v3-share2 t=42",
            "BVAGENT SERVICE start t=50",
            "BVAGENT SERVICE alive t=75",
            "BVAGENT SERVICE timeout CLIPGET t=90"
        ])
        XCTAssertEqual(events, [
            .ready(host: "WIN11 v3-share2", tMs: 42),
            .serviceStart(tMs: 50),
            .aliveHeartbeat(tMs: 75),
            .timeout(kind: "CLIPGET", tMs: 90)
        ])
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
        XCTAssertEqual(backend.targetDiskPath, cfg.bundlePath + "/disks/hvf-target.raw")
        XCTAssertEqual(backend.uefiVarsPath, cfg.bundlePath + "/metadata/hvf-vars.fd")
        XCTAssertEqual(backend.evidenceDir, cfg.bundlePath + "/logs/hvf")
        XCTAssertEqual(backend.ctlFilePath, cfg.bundlePath + "/metadata/hvf.ctl")
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
