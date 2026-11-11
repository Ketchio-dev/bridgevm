import Foundation
import XCTest
@testable import BridgeVMControl

/// 가상 머신 > Windows 앱: the agent's STARTAPPS rows are guest bytes, parsed row by row; STARTAPP goes only to an
/// agent whose READY carries `startapps`, with an AppID the agent takes.
final class HvfGuestStartAppsTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1000)
    private let calculator = "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
    private let notepad = "{6D809377-6AF0-444B-8957-A3773F02200E}\\Notepad++\\notepad++.exe"

    private func b64(_ text: String) -> String { Data(text.utf8).base64EncodedString() }
    private func row(_ name: String, _ id: String) -> String { b64(name) + "|" + b64(id) }

    func testRowsAreStrictBase64UTF8WithAnAppIDTheAgentTakes() {
        XCTAssertEqual(HvfGuestStartApp(row: row("Calculator", calculator)), HvfGuestStartApp(name: "Calculator", appID: calculator))
        XCTAssertEqual(HvfGuestStartApp(row: row(" Note\u{7}pad++\u{202E}\u{2028} ", notepad))?.name, "Notepad++",
                       "control, format and separator characters never reach a menu item")
        let malformed = [
            "", "|", b64("Calculator"), row("Calculator", calculator) + "|" + b64("x"), "QQ|" + b64(calculator),
            "Q Q==|" + b64(calculator), b64("Calculator") + "|" + Data([0x41, 0xC3, 0x28]).base64EncodedString(),
            row("", calculator), row(" \u{7} ", calculator), row(String(repeating: "n", count: 257), calculator),
            row("Calculator", "x\" & calc & \""), row("Calculator", "..\\cmd.exe"), row("Calculator", "C:\\Windows\\cmd.exe"),
            row("Calculator", ""), row(String(repeating: "n", count: 200), String(repeating: "a", count: 6000)),
        ]
        for line in malformed { XCTAssertNil(HvfGuestStartApp(row: line), line) }
    }

    /// The agent's Test-BvStartAppId, the same cases as tests/integration/agent-start-apps-contract.ps1.
    func testAppIDRuleMatchesTheAgentsAndTheLaunchIsOneProtocolLine() throws {
        let guid = "{6D809377-6AF0-444B-8957-A3773F02200E}"
        for id in [calculator, notepad, "\(guid)\\Mozilla Firefox\\firefox.exe", "{7C5A40EF-A0FB-4BFC-874A-C0F2E0B9FA8E}\\Steam (x86)\\steam.exe",
                   "windows.immersivecontrolpanel_cw5n1h2txyewy!microsoft.windows.immersivecontrolpanel", "Microsoft.Windows.Explorer",
                   "MSEdge", "\(guid)\\카카오\\App~1 [beta] #2 @home.exe", String(repeating: "a", count: 1024)] {
            XCTAssertTrue(HvfGuestStartApps.isAppID(id), id)
            let command = try XCTUnwrap(HvfGuestStartApps.launchCommand(id))
            XCTAssertEqual(command, "STARTAPP " + b64(id))
            XCTAssertEqual(try HvfGuestCommand.normalize(command).get(), command)
        }
        for id in ["", " ", "x\"", "x'", "x`y", "x\ty", "x\ny", "x\ry", "x\u{0}y", "x\u{1B}y", "x\u{202E}y", "x\u{FF02}y", "a..b",
                   "..\\Windows\\System32\\cmd.exe", "\(guid)\\..\\..\\cmd.exe", "C:\\Windows\\System32\\cmd.exe", "\\\\server\\share\\x.exe",
                   "a\\\\b", "\\x", "x\\", " x", "x ", "https://example.com/", "\(calculator),/root,C:\\", "x\" & calc & \"", "a&b", "a|b",
                   "a;b", "%COMSPEC%", "a^b", "a<b", "a>b", "$env:x", "a/b", "a*b", "a?b", "a=b", String(repeating: "a", count: 1025)] {
            XCTAssertFalse(HvfGuestStartApps.isAppID(id), id.debugDescription)
            XCTAssertNil(HvfGuestStartApps.launchCommand(id), id.debugDescription)
        }
        XCTAssertTrue(HvfGuestCommand.relayProtocolVerbs.isSuperset(of: ["STARTAPPS", "STARTAPP"]), "the relay sends them raw, never as RUN")
    }

    func testOnlyTheQuestionsOwnFrameAnswersItSortedAndOncePerAppID() throws {
        var request = HvfGuestStartAppsRequest(now: now)
        XCTAssertTrue(request.command.hasPrefix("STARTAPPS "))
        XCTAssertNotNil(UUID(uuidString: String(request.command.dropFirst("STARTAPPS ".count))))
        let other = "STARTAPPS \(UUID().uuidString)"
        XCTAssertNil(request.consume(lines: [
            row("Before", "Before.App"), "BVAGENT CMD \(other) exit=0", row("Other", "Other.App"), "BVAGENT END \(other)",
        ], now: now))
        XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0", row("Notepad++", notepad) + "\r",
                                             "not a row", row("Calculator", calculator)], now: now))
        let apps = try XCTUnwrap(request.consume(lines: [row("Calculator again", calculator.uppercased()), row("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App"),
                                                         "BVAGENT END \(request.command)\r"], now: now)).get()
        XCTAssertEqual(apps.map(\.name), ["Calculator", "Notepad++", "Paint"])
        XCTAssertEqual(apps.first?.appID, calculator)

        var empty = HvfGuestStartAppsRequest(now: now)
        XCTAssertEqual(try XCTUnwrap(empty.consume(lines: ["BVAGENT CMD \(empty.command) exit=0", "", "BVAGENT END \(empty.command)"], now: now)).get(), [])
    }

    func testAFailedOldRestartedOrSilentAgentEndsTheQuestion() {
        for exit in ["1", "255", "-1"] {
            var request = HvfGuestStartAppsRequest(now: now)
            XCTAssertEqual(request.consume(lines: ["BVAGENT CMD \(request.command) exit=\(exit)", "BVSTARTAPPS unavailable"], now: now),
                           .failure(.unavailable), exit)
        }
        for marker in ["BVAGENT READY host=WIN t=1", "BVAGENT re-READY WIN t=2", "BVAGENT SERVICE start t=2", "PSCI SYSTEM_RESET: reboot 1/8"] {
            var request = HvfGuestStartAppsRequest(now: now)
            XCTAssertNil(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0"], now: now))
            XCTAssertEqual(request.consume(lines: [marker], now: now), .failure(.restarted), marker)
        }
        var request = HvfGuestStartAppsRequest(now: now)
        XCTAssertNil(request.consume(lines: [], now: now.addingTimeInterval(29)))
        XCTAssertEqual(request.consume(lines: [], now: now.addingTimeInterval(30)), .failure(.expired))
    }

    func testRowsPastTheAppAndByteLimitsAreDropped() throws {
        var request = HvfGuestStartAppsRequest(now: now)
        let rows = (0...HvfGuestStartApps.maximumApps).map { row(String(format: "App %04d", $0), String(format: "Vendor.App%04d_abc!App", $0)) }
        let apps = try XCTUnwrap(request.consume(lines: ["BVAGENT CMD \(request.command) exit=0"] + rows + ["BVAGENT END \(request.command)"], now: now)).get()
        XCTAssertEqual(apps.count, HvfGuestStartApps.maximumApps)

        var large = HvfGuestStartAppsRequest(now: now)
        let filler = Array(repeating: String(repeating: "x", count: 8000), count: HvfGuestStartApps.maximumBytes / 8000 + 1)
        let kept = try XCTUnwrap(large.consume(lines: ["BVAGENT CMD \(large.command) exit=0", row("Calculator", calculator)] + filler
                                               + [row("Paint", "Microsoft.Paint_8wekyb3d8bbwe!App"), "BVAGENT END \(large.command)"], now: now)).get()
        XCTAssertEqual(kept.map(\.name), ["Calculator"], "past the byte limit nothing more is kept")
    }

    func testALaunchEndsWithItsOwnAnswer() throws {
        let app = HvfGuestStartApp(name: "Calculator", appID: calculator)
        let launch = try XCTUnwrap(HvfGuestStartAppLaunch(app: app, now: now))
        XCTAssertEqual(launch.command, "STARTAPP " + b64(calculator))
        let other = "STARTAPP " + b64(notepad)
        XCTAssertNil(launch.consume(lines: ["BVAGENT CMD \(other) exit=1"], now: now))
        XCTAssertEqual(launch.consume(lines: ["BVAGENT CMD \(launch.command) exit=0"], now: now), .opened)
        XCTAssertEqual(launch.consume(lines: ["BVAGENT CMD \(launch.command) exit=1"], now: now), .refused)
        XCTAssertEqual(launch.consume(lines: ["BVAGENT SERVICE start t=9"], now: now), .gone)
        XCTAssertEqual(launch.consume(lines: [], now: now.addingTimeInterval(30)), .gone)
        XCTAssertNil(HvfGuestStartAppLaunch(app: HvfGuestStartApp(name: "x", appID: "x\" & calc"), now: now))
    }

    /// Up to 40 entries, apps before Windows' own tools, documents and uninstallers, shown alphabetically.
    func testTheMenuShowsFortyAppsBeforeWindowsToolsAlphabetically() {
        let tools = ["Command Prompt": "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\cmd.exe",
                     "Aa Character Map": "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\charmap.exe",
                     "Aa Readme": "{6D809377-6AF0-444B-8957-A3773F02200E}\\Vendor\\readme.txt",
                     "Aa Uninstall Vendor": "{6D809377-6AF0-444B-8957-A3773F02200E}\\Vendor\\uninstall.exe"]
        let apps = (1...38).map { HvfGuestStartApp(name: String(format: "App %02d", 39 - $0), appID: String(format: "Vendor.App%02d_abc!App", $0)) }
            + tools.map { HvfGuestStartApp(name: $0.key, appID: $0.value) }
        let menu = HvfGuestStartApps.menuApps(apps)
        XCTAssertEqual(menu.count, 40)
        XCTAssertEqual(menu.map(\.name), ["Aa Character Map", "Aa Readme"] + (1...38).map { String(format: "App %02d", $0) },
                       "38 apps, then the first two others alphabetically, all shown alphabetically")
        XCTAssertEqual(HvfGuestStartApps.menuApps(Array(apps.prefix(3))).map(\.name), ["App 36", "App 37", "App 38"])
    }

    func testTheReadyMarker() {
        XCTAssertTrue(HvfGuestStartApps.advertised("WIN v3-share2 putdata bigfile startapps agent=0123456789abcdef update=pending"))
        XCTAssertTrue(HvfGuestStartApps.advertised("BVAGENT re-READY host=WIN v3-share2 startapps (handshake) t=4"))
        for old in ["WIN v3-share2 putdata bigfile", "WIN v3-share2 putdata bigfile agent=0123456789abcdef", "WIN startappsx", "STARTAPPS"] {
            XCTAssertFalse(HvfGuestStartApps.advertised(old), old)
        }
    }
}

#if canImport(AppKit)
/// The submenu's state follows the session in front: no VM, a VM not yet ready, a paused one, or an agent without the
/// marker each say why in one sentence; the marker comes from READY and from an agent restart's re-READY.
@MainActor
final class HvfGuestStartAppsMenuTests: XCTestCase {
    func testWithoutTheMarkerOrARunningGuestTheSubmenuSaysWhy() {
        XCTAssertEqual(HvfVirtualMachineMenuState(session: nil).windowsAppsUnavailable, HvfGuestStartApps.notRunning)
        let session = makeSession()
        XCTAssertEqual(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, HvfGuestStartApps.notRunning, "stopped")
        session.connectionState = .booting
        XCTAssertEqual(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, HvfGuestStartApps.notReady)

        for event in BvAgentEvent.parse(lines: ["BVAGENT READY host=WIN v3-share2 putdata bigfile agent=0123456789abcdef t=5"]) {
            session.handle(event)
        }
        XCTAssertEqual(session.connectionState, .connected(host: "WIN v3-share2 putdata bigfile agent=0123456789abcdef"))
        XCTAssertEqual(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, HvfGuestStartApps.notReady, "no service yet")
        session.serviceStarted = true
        XCTAssertEqual(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, HvfGuestStartApps.agentTooOld,
                       "an older agent is never asked")
        let old = HvfGuestStartAppsController(session: session)
        XCTAssertFalse(old.refresh())
        XCTAssertFalse(old.launch(HvfGuestStartApp(name: "Calculator", appID: "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")))
        XCTAssertFalse(FileManager.default.fileExists(atPath: session.config.ctlFilePath), "nothing went to the guest")

        for event in BvAgentEvent.parse(lines: ["BVAGENT re-READY WIN v3-share2 putdata bigfile startapps agent=fedcba9876543210 t=9"]) {
            session.handle(event)
        }
        XCTAssertNil(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, "the updated agent takes the verbs")
        session.pause = .paused
        XCTAssertEqual(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, HvfGuestStartApps.paused)
        session.pause = .running
        session.handle(.ready(host: "WIN v3-share2", tMs: 12))
        XCTAssertEqual(HvfVirtualMachineMenuState(session: session).windowsAppsUnavailable, HvfGuestStartApps.agentTooOld,
                       "a rolled-back agent loses it again")
    }

    /// The list goes out as `STARTAPPS <id>`, its frame in run.log fills the menu, a stale list is asked again only
    /// after HvfGuestStartAppsController.staleAfter, and a refused launch tells the user.
    func testTheControllerAsksLaunchesAndReportsARefusal() throws {
        let session = makeSession(marker: true)
        let controller = HvfGuestStartAppsController(session: session)
        var notices: [String] = []
        controller.onNotice = { notices.append($0) }
        XCTAssertEqual(controller.statusText, HvfGuestStartApps.notLoaded)
        XCTAssertTrue(controller.refresh())
        XCTAssertEqual(controller.status, .loading)
        XCTAssertFalse(controller.refresh(), "one question at a time")
        let question = try XCTUnwrap(sent().last)
        XCTAssertTrue(question.hasPrefix("STARTAPPS "), question)
        try log(["BVAGENT CMD \(question) exit=0",
                 Data("Calculator".utf8).base64EncodedString() + "|" + Data("Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".utf8).base64EncodedString(),
                 "garbage", "BVAGENT END \(question)"], session)
        controller.poll()
        XCTAssertEqual(controller.status, .loaded)
        let calculator = try XCTUnwrap(controller.menuApps.first)
        XCTAssertEqual(controller.menuApps, [HvfGuestStartApp(name: "Calculator", appID: "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")])

        controller.loadIfStale()
        XCTAssertEqual(sent().filter { $0.hasPrefix("STARTAPPS ") }.count, 1, "a fresh list is not asked again")
        controller.loadIfStale(now: Date().addingTimeInterval(HvfGuestStartAppsController.staleAfter + 1))
        XCTAssertEqual(sent().filter { $0.hasPrefix("STARTAPPS ") }.count, 2)
        let again = try XCTUnwrap(sent().last)
        try log(["BVAGENT CMD \(again) exit=1", "BVSTARTAPPS unavailable", "BVAGENT END \(again)"], session)
        controller.poll()
        XCTAssertEqual(controller.status, .failed(HvfGuestStartApps.listUnavailable))
        XCTAssertEqual(controller.apps.count, 1, "the last list stays")

        XCTAssertTrue(controller.launch(calculator))
        let launch = try XCTUnwrap(sent().last)
        XCTAssertEqual(launch, "STARTAPP " + Data(calculator.appID.utf8).base64EncodedString())
        try log(["BVAGENT CMD \(launch) exit=1", "BVSTARTAPP failed", "BVAGENT END \(launch)"], session)
        controller.poll()
        XCTAssertEqual(notices, [HvfGuestStartApps.launchFailed("Calculator")])
        XCTAssertTrue(controller.launch(calculator))
        try log(["BVAGENT CMD \(launch) exit=0", "BVSTARTAPP ok", "BVAGENT END \(launch)"], session)
        controller.poll()
        XCTAssertEqual(notices.count, 1, "an opened app says nothing")
    }

    func testTheMenuOpensTheAllAppsSheet() {
        let display = HvfDisplayWindowState(session: makeSession())
        display.perform(.allApps)
        XCTAssertTrue(display.showsAllApps)
        XCTAssertFalse(display.showsWindowInventory)
    }

    private var ctlPath = ""

    private func makeSession(marker: Bool = false) -> HvfEngineSession {
        let root = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("bridgevm-startapps-" + UUID().uuidString)
        try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        ctlPath = config.ctlFilePath
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        if marker {
            session.handle(.ready(host: "WIN v3-share2 putdata bigfile startapps agent=0123456789abcdef", tMs: 1))
            session.serviceStarted = true
        }
        return session
    }

    private func sent() -> [String] {
        ((try? String(contentsOfFile: ctlPath, encoding: .utf8)) ?? "").split(separator: "\n").map(String.init)
    }

    private func log(_ lines: [String], _ session: HvfEngineSession) throws {
        let url = URL(fileURLWithPath: session.config.evidenceDir).appendingPathComponent("run.log")
        if !FileManager.default.fileExists(atPath: url.path) { FileManager.default.createFile(atPath: url.path, contents: nil) }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data((lines.joined(separator: "\n") + "\n").utf8))
    }
}
#endif
