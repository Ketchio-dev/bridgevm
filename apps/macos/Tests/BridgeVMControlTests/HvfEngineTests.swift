import XCTest
@testable import BridgeVMControl

final class HvfEngineConfigTests: XCTestCase {
    func testLibraryWindowsVMBuildsTurnkeyControlConfiguration() {
        let vm = VMConfig(
            id: "win11", name: "Windows 11", displayName: "Windows 11",
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: "/library/win11/bundle.vmbridge", runnerPath: "",
            launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "win11", displayWidth: 1280, displayHeight: 800,
            installPending: false, isoPath: nil, diskPath: "/custom/windows.raw",
            memMiB: 8192, cpuCount: 8
        )

        let config = HvfEngineConfig.libraryVM(vm)

        XCTAssertEqual(config?.targetDiskPath, "/custom/windows.raw")
        XCTAssertEqual(config?.uefiVarsPath, "/library/win11/bundle.vmbridge/metadata/hvf-vars.fd")
        XCTAssertEqual(config?.evidenceDir, "/library/win11/bundle.vmbridge/logs/hvf")
        XCTAssertEqual(config?.ctlFilePath, "/library/win11/bundle.vmbridge/metadata/hvf.ctl")
        XCTAssertEqual(config?.ramMiB, 8192)
        XCTAssertEqual(config?.smpCpus, 8)
        XCTAssertEqual(config?.virtioNet, true)
        XCTAssertEqual(config?.virtioGpu3d, true)
        XCTAssertNil(config?.watchdogMs)
    }

    func testLibraryConfigurationRejectsNonHVFVM() {
        let vm = VMConfig(
            id: "linux", name: "Linux", displayName: "Linux", backendKind: "fast-vz",
            bootMode: "direct-kernel", bundlePath: "/library/linux", runnerPath: "",
            launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: "linux", displayWidth: 1280, displayHeight: 800
        )
        XCTAssertNil(HvfEngineConfig.libraryVM(vm))
    }

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
                                  virtioGpu3d: false,
                                  nvmeBufferedIO: false,
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
            "--agent-service-command", "whoami",
            "--display-export-ppm", "/tmp/evidence/display.ppm",
            "--display-export-ms", "500",
            "--enable-xhci",
            "--input-control", "/tmp/evidence/input.ctl"
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
                                  virtioGpu3d: true,
                                  nvmeBufferedIO: true,
                                  ctlFilePath: "/tmp/evidence/ctl")
        let args = cfg.wrapperArguments()
        XCTAssertTrue(args.contains("--virtio-net"))
        XCTAssertTrue(args.contains("--agent-clipboard-sync"))
        XCTAssertTrue(args.contains("--release"))
        XCTAssertTrue(args.contains("--skip-build"))
        XCTAssertTrue(args.contains("--nvme-buffered-io"))
        XCTAssertTrue(args.contains("--virtio-gpu-3d"))
        XCTAssertEqual(value(after: "--virtio-gpu-device-id", in: args), "1050")
        XCTAssertEqual(value(after: "--gpu-trace-protocol", in: args), "virgl")
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
                                  virtioGpu3d: false,
                                  nvmeBufferedIO: false,
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
                                  virtioGpu3d: false,
                                  nvmeBufferedIO: false,
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

final class HvfHostKeyCommandTests: XCTestCase {
    func testMapsHostNavigationAndEditingKeys() {
        let cases: [(String, HvfHostKeyCommand)] = [
            ("\u{1b}", .key("esc")), ("\u{7f}", .key("backspace")),
            ("\u{f728}", .key("delete")), ("\u{f700}", .key("up")),
            ("\u{f701}", .key("down")), ("\u{f702}", .key("left")),
            ("\u{f703}", .key("right")), ("\u{f729}", .key("home")),
            ("\u{f72b}", .key("end")), ("\u{f72c}", .key("pageup")),
            ("\u{f72d}", .key("pagedown")), ("\t", .key("tab")),
            ("\r", .key("enter"))
        ]
        for (characters, expected) in cases {
            XCTAssertEqual(HvfHostKeyCommand.resolve(characters: characters), expected)
        }
    }

    func testMapsTextAndSecureAttentionWhileIgnoringHostShortcuts() {
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "A"), .text("A"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(
            characters: "\u{7f}", modifiers: [.control, .option]
        ), .key("ctrl+alt+delete"))
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "c", modifiers: .command), .ignored)
        XCTAssertEqual(HvfHostKeyCommand.resolve(characters: "", modifiers: []), .ignored)
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

    @MainActor
    func testAttachesToRunningVMAndUsesItsGracefulControlChannel() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let evidence = temp.appendingPathComponent("evidence", isDirectory: true)
        let ctl = temp.appendingPathComponent("metadata/hvf.ctl")
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: ctl.deletingLastPathComponent(), withIntermediateDirectories: true)
        try "BVAGENT READY host=WIN11 t=10\nBVAGENT SERVICE start t=20\n".write(
            to: evidence.appendingPathComponent("run.log"), atomically: true, encoding: .utf8
        )
        try Data().write(to: ctl)
        let config = HvfEngineConfig(
            targetDiskPath: temp.appendingPathComponent("windows.raw").path,
            uefiVarsPath: temp.appendingPathComponent("vars.fd").path,
            evidenceDir: evidence.path,
            watchdogMs: nil,
            ramMiB: 6144,
            smpCpus: 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: true,
            virtioGpu3d: true,
            nvmeBufferedIO: true,
            ctlFilePath: ctl.path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in true }

        XCTAssertTrue(session.attachToRunningVM())
        XCTAssertEqual(session.connectionState, .connected(host: "WIN11"))

        session.stop()

        XCTAssertEqual(session.connectionState, .stopping)
        XCTAssertEqual(try String(contentsOf: ctl, encoding: .utf8), "shutdown.exe /p /f\n")
        XCTAssertTrue(session.events.contains(.unknown("graceful guest shutdown requested")))
    }

    @MainActor
    func testStartAttachesInsteadOfLaunchingDuplicateVM() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let evidence = temp.appendingPathComponent("evidence", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        let config = HvfEngineConfig(
            targetDiskPath: temp.appendingPathComponent("windows.raw").path,
            uefiVarsPath: temp.appendingPathComponent("vars.fd").path,
            evidenceDir: evidence.path,
            watchdogMs: nil,
            ramMiB: 6144,
            smpCpus: 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: true,
            virtioGpu3d: true,
            nvmeBufferedIO: true,
            ctlFilePath: temp.appendingPathComponent("hvf.ctl").path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in true }

        session.start()

        XCTAssertEqual(session.connectionState, .booting)
        XCTAssertTrue(session.events.contains(.unknown(
            "attached to the already running HVF engine; duplicate launch prevented"
        )))
    }

    @MainActor
    func testTextInputPreservesPrintableASCIIAndChunksLongCommands() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        let input = temp.appendingPathComponent("input.ctl")
        try Data().write(to: input)
        let config = HvfEngineConfig(
            targetDiskPath: temp.appendingPathComponent("windows.raw").path,
            uefiVarsPath: temp.appendingPathComponent("vars.fd").path,
            evidenceDir: temp.path,
            watchdogMs: nil,
            ramMiB: 6144,
            smpCpus: 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: true,
            virtioGpu3d: true,
            nvmeBufferedIO: true,
            ctlFilePath: temp.appendingPathComponent("hvf.ctl").path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in false }

        session.sendText("Ab c!@?," + String(repeating: "x", count: 32) + "끝")

        let lines = try String(contentsOf: input, encoding: .utf8)
            .split(separator: "\n").map(String.init)
        XCTAssertEqual(lines.count, 2)
        XCTAssertEqual(lines[0], "KEY text-hex:4162206321403f2c" + String(repeating: "78", count: 24))
        XCTAssertEqual(lines[1], "KEY text-hex:" + String(repeating: "78", count: 8))
    }

    @MainActor
    func testSpecialKeyInputWritesNamedHIDActions() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        let input = temp.appendingPathComponent("input.ctl")
        try Data().write(to: input)
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: temp.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: true,
            nvmeBufferedIO: true, ctlFilePath: temp.appendingPathComponent("hvf.ctl").path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in false }

        for key in ["esc", "backspace", "delete", "left", "up", "down", "right", "ctrl+alt+delete"] {
            session.sendKey(key)
        }

        XCTAssertEqual(
            try String(contentsOf: input, encoding: .utf8),
            "KEY esc\nKEY backspace\nKEY delete\nKEY left\nKEY up\nKEY down\nKEY right\nKEY ctrl+alt+delete\n"
        )
    }

    @MainActor
    func testLiveInputHandleReseeksAfterConsumerCompaction() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let input = temp.appendingPathComponent("input.ctl")
        try Data().write(to: input)
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: temp.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: true,
            nvmeBufferedIO: true, ctlFilePath: temp.appendingPathComponent("hvf.ctl").path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in false }
        session.sendKey("esc")

        let consumer = try FileHandle(forWritingTo: input)
        try consumer.truncate(atOffset: 0)
        try consumer.close()
        session.sendKey("delete")

        XCTAssertEqual(try String(contentsOf: input, encoding: .utf8), "KEY delete\n")
        let size = try XCTUnwrap(
            FileManager.default.attributesOfItem(atPath: input.path)[.size] as? NSNumber
        )
        XCTAssertEqual(size.uint64Value, 11)
    }

    @MainActor
    func testControlInputRejectsMultilineWithoutWritingChannel() throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        let ctl = temp.appendingPathComponent("hvf.ctl")
        let config = HvfEngineConfig(
            targetDiskPath: "target", uefiVarsPath: "vars", evidenceDir: temp.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: true,
            nvmeBufferedIO: true, ctlFilePath: ctl.path
        )
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in false }

        XCTAssertFalse(session.sendCtl("one\ntwo"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: ctl.path))
        XCTAssertTrue(session.events.contains(.unknown(
            "control command rejected: \(HvfGuestCommandError.multiline.message)"
        )))
    }

    private func makeWrapper(at root: URL) throws {
        let scripts = root.appendingPathComponent("scripts", isDirectory: true)
        try FileManager.default.createDirectory(at: scripts, withIntermediateDirectories: true)
        let wrapper = scripts.appendingPathComponent("run-hvf-windows-installed-boot.sh")
        try "#!/usr/bin/env bash\n".write(to: wrapper, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: wrapper.path)
    }
}

#if canImport(AppKit)
final class HvfDisplayCoordinatesTests: XCTestCase {
    func testAspectFitCoordinatesRejectLetterboxAndMapCorners() {
        let view = CGSize(width: 1000, height: 1000)
        let image = CGSize(width: 1280, height: 800)
        XCTAssertNil(HvfDisplayCoordinates.absolutePointer(
            location: CGPoint(x: 500, y: 100), viewSize: view, imageSize: image
        ))
        XCTAssertEqual(
            HvfDisplayCoordinates.absolutePointer(
                location: CGPoint(x: 0, y: 187.5), viewSize: view, imageSize: image
            )?.x,
            0
        )
        let bottomRight = HvfDisplayCoordinates.absolutePointer(
            location: CGPoint(x: 1000, y: 812.5), viewSize: view, imageSize: image
        )
        XCTAssertEqual(bottomRight?.x, 32_767)
        XCTAssertEqual(bottomRight?.y, 32_767)
    }
}

final class HvfScrollDeltaTests: XCTestCase {
    func testMapsTrackpadFractionsAndMouseWheelStepsToNonzeroHIDDelta() {
        XCTAssertEqual(HvfScrollDelta.hid(from: 0.1), 1)
        XCTAssertEqual(HvfScrollDelta.hid(from: -0.1), -1)
        XCTAssertEqual(HvfScrollDelta.hid(from: 4.6), 5)
    }

    func testClampsToSignedHIDReportRangeAndRejectsInvalidValues() {
        XCTAssertEqual(HvfScrollDelta.hid(from: 1_000), 127)
        XCTAssertEqual(HvfScrollDelta.hid(from: -1_000), -127)
        XCTAssertNil(HvfScrollDelta.hid(from: 0))
        XCTAssertNil(HvfScrollDelta.hid(from: .infinity))
    }
}
#endif

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

final class HvfScreenshotSourceTests: XCTestCase {
    private func temporaryDirectory() throws -> URL {
        let url = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    func testFingerprintIsStableUntilFileIsReplaced() throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let file = dir.appendingPathComponent("display.ppm")
        try Data("first".utf8).write(to: file)
        let first = try XCTUnwrap(HvfScreenshotSource.fingerprint(of: file))

        XCTAssertEqual(HvfScreenshotSource.fingerprint(of: file), first)
        try Data("other".utf8).write(to: file, options: .atomic)

        XCTAssertNotEqual(HvfScreenshotSource.fingerprint(of: file), first)
    }

    func testLiveDisplayTakesPriorityAndRamfbFallsBackToNewestFrame() throws {
        let dir = try temporaryDirectory()
        defer { try? FileManager.default.removeItem(at: dir) }
        let ramfb = dir.appendingPathComponent("ramfb", isDirectory: true)
        try FileManager.default.createDirectory(at: ramfb, withIntermediateDirectories: true)
        let old = ramfb.appendingPathComponent("old.ppm")
        let newest = ramfb.appendingPathComponent("new.ppm")
        try Data("old".utf8).write(to: old)
        try Data("new".utf8).write(to: newest)
        try FileManager.default.setAttributes(
            [.modificationDate: Date(timeIntervalSinceNow: -10)],
            ofItemAtPath: old.path
        )

        XCTAssertEqual(HvfScreenshotSource.resolve(in: dir)?.0.lastPathComponent, newest.lastPathComponent)

        let live = dir.appendingPathComponent("display.ppm")
        try Data("live".utf8).write(to: live)
        XCTAssertEqual(HvfScreenshotSource.resolve(in: dir)?.0.lastPathComponent, live.lastPathComponent)
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
    func testGuestCommandValidationRejectsProtocolBreakingInput() {
        XCTAssertEqual(try? HvfGuestCommand.normalize("  whoami  ").get(), "whoami")
        XCTAssertEqual(
            HvfGuestCommand.normalize("one\ntwo").failure,
            .multiline
        )
        XCTAssertEqual(
            HvfGuestCommand.normalize(String(repeating: "x", count: HvfGuestCommand.maximumBytes + 1)).failure,
            .tooLong(actual: HvfGuestCommand.maximumBytes + 1, maximum: HvfGuestCommand.maximumBytes)
        )
    }

    func testPathKindAndDisplayNameDerivation() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg, processIsRunning: { _ in true })

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
        let backend = HvfWindowsBackend(cfg, processIsRunning: { _ in true })
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

    func testRunInGuestRejectsStoppedVMWithoutWritingControlFile() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg, processIsRunning: { _ in false })

        let result = backend.runInGuest("foo")

        XCTAssertEqual(result.output, "HVF VM이 실행 중이 아닙니다.")
        XCTAssertEqual(result.code, -1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: backend.ctlFilePath))
    }

    func testRunInGuestRejectsMultilineCommandBeforeCheckingVMOrWritingControlFile() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg, processIsRunning: { _ in true })

        let result = backend.runInGuest("one\ntwo")

        XCTAssertEqual(result.output, HvfGuestCommandError.multiline.message)
        XCTAssertEqual(result.code, -1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: backend.ctlFilePath))
    }

    func testRunInGuestReturnsImmediatelyWhenVMExitsWhileWaiting() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let lock = NSLock()
        var checks = 0
        let backend = HvfWindowsBackend(cfg, processIsRunning: { _ in
            lock.lock()
            defer { lock.unlock() }
            checks += 1
            return checks == 1
        })

        let started = Date()
        let result = backend.runInGuest("foo")

        XCTAssertLessThan(Date().timeIntervalSince(started), 1)
        XCTAssertEqual(result.output, "HVF 게스트 연결이 명령 실행 중 종료되었습니다: foo")
        XCTAssertEqual(result.code, -1)
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

    func testConcurrentGuestCommandsWaitForPriorReplyBeforeAppending() throws {
        let dir = try makeTempDir()
        defer { try? FileManager.default.removeItem(at: dir) }
        let cfg = makeConfig(bundlePath: dir.appendingPathComponent("bundle.vmbridge").path)
        let backend = HvfWindowsBackend(cfg, processIsRunning: { _ in true })
        let log = URL(fileURLWithPath: backend.evidenceDir).appendingPathComponent("run.log")
        let group = DispatchGroup()
        for command in ["first", "second"] {
            group.enter()
            DispatchQueue.global().async {
                _ = backend.runInGuest(command)
                group.leave()
            }
        }

        let firstCommand = try waitForControlLines(backend, count: 1).first.map(String.init)!
        usleep(150_000)
        XCTAssertEqual(try controlLines(backend).count, 1)
        try FileManager.default.createDirectory(atPath: backend.evidenceDir, withIntermediateDirectories: true)
        try "BVAGENT CMD \(firstCommand) exit=0\nfirst-ok\nBVAGENT END \(firstCommand)\n"
            .write(to: log, atomically: true, encoding: .utf8)

        let commands = try waitForControlLines(backend, count: 2).map(String.init)
        let secondCommand = commands[1]
        try """
        BVAGENT CMD \(firstCommand) exit=0
        first-ok
        BVAGENT END \(firstCommand)
        BVAGENT CMD \(secondCommand) exit=0
        second-ok
        BVAGENT END \(secondCommand)

        """.write(to: log, atomically: true, encoding: .utf8)

        XCTAssertEqual(group.wait(timeout: .now() + 2), .success)
        XCTAssertEqual(Set(commands), Set(["first", "second"]))
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

    private func controlLines(_ backend: HvfWindowsBackend) throws -> [Substring] {
        try String(contentsOfFile: backend.ctlFilePath, encoding: .utf8).split(separator: "\n")
    }

    private func waitForControlLines(_ backend: HvfWindowsBackend, count: Int) throws -> [Substring] {
        let deadline = Date().addingTimeInterval(2)
        while Date() < deadline {
            if FileManager.default.fileExists(atPath: backend.ctlFilePath) {
                let lines = try controlLines(backend)
                if lines.count >= count { return lines }
            }
            usleep(10_000)
        }
        XCTFail("timed out waiting for \(count) HVF control lines")
        return []
    }

    private func makeConfig(bundlePath: String, diskPath: String? = nil) -> VMConfig {
        VMConfig(id: "win-hvf", name: "Win HVF", displayName: "Windows Preview", backendKind: "hvf-engine",
                 bootMode: "windows-hvf", bundlePath: bundlePath, runnerPath: "", launchSpecPath: "",
                 handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "",
                 guestName: "win-hvf", displayWidth: 1280, displayHeight: 800,
                 diskPath: diskPath, memMiB: 4096, cpuCount: 1)
    }
}

final class HvfCommandReplyReaderTests: XCTestCase {
    func testConsumesAppendedReplyIncrementallyIncludingPartialLines() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let log = dir.appendingPathComponent("run.log")
        try Data("old log\n".utf8).write(to: log)
        let reader = HvfCommandReplyReader(command: "foo", offset: 8)

        let handle = try FileHandle(forWritingTo: log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("BVAGENT CMD foo exit=7\nfirst\nsec".utf8))
        XCTAssertNil(reader.readReply(from: log))
        try handle.write(contentsOf: Data("ond\nBVAGENT END foo\n".utf8))
        try handle.close()

        let reply = try XCTUnwrap(reader.readReply(from: log))
        XCTAssertEqual(reply.output, "first\nsecond")
        XCTAssertEqual(reply.code, 7)
    }

    func testResetsAfterLogTruncation() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let log = dir.appendingPathComponent("run.log")
        try Data(repeating: 120, count: 100).write(to: log)
        let reader = HvfCommandReplyReader(command: "foo", offset: 100)
        try Data("BVAGENT CMD foo exit=0\nok\nBVAGENT END foo\n".utf8).write(to: log, options: .atomic)

        let reply = try XCTUnwrap(reader.readReply(from: log))
        XCTAssertEqual(reply.output, "ok")
        XCTAssertEqual(reply.code, 0)
    }
}

final class HvfIncrementalMarkerReaderTests: XCTestCase {
    func testFindsMarkerAcrossAppendsAndChunkBoundary() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let log = dir.appendingPathComponent("run.log")
        try Data("noise\nBVAGENT SERV".utf8).write(to: log)
        let reader = HvfIncrementalMarkerReader(marker: "BVAGENT SERVICE start")
        XCTAssertFalse(reader.containsMarker(in: log))

        let handle = try FileHandle(forWritingTo: log)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("ICE start t=1\n".utf8))
        try handle.close()
        XCTAssertTrue(reader.containsMarker(in: log))
        XCTAssertTrue(reader.containsMarker(in: log))
    }

    func testResetAllowsASecondLogGeneration() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let log = dir.appendingPathComponent("run.log")
        let reader = HvfIncrementalMarkerReader(marker: "ready")
        try Data("ready\n".utf8).write(to: log)
        XCTAssertTrue(reader.containsMarker(in: log))

        reader.reset()
        try Data("booting\n".utf8).write(to: log, options: .atomic)
        XCTAssertFalse(reader.containsMarker(in: log))
        try Data("ready\n".utf8).write(to: log, options: .atomic)
        XCTAssertTrue(reader.containsMarker(in: log))
    }
}

private extension Result {
    var failure: Failure? {
        guard case let .failure(error) = self else { return nil }
        return error
    }
}
