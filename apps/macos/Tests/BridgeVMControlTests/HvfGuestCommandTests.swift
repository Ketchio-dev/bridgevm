import XCTest
@testable import BridgeVMControl

final class HvfGuestCommandTests: XCTestCase {
    func testNormalizeRejectsEveryLineBreakIncludingCRLFGraphemes() {
        for command in ["echo one\r\necho two", "echo one\recho two", "echo one\necho two", "echo one\0two", "echo one\r\n"] {
            if case .failure(.multiline) = HvfGuestCommand.normalize(command) { continue }
            XCTFail("accepted a multi-line command: \(command.debugDescription)")
        }
    }

    func testShellNormalizationRefusesOnlyRelayVerbsAsTheFirstToken() {
        for verb in HvfGuestCommand.relayProtocolVerbs {
            XCTAssertEqual(HvfGuestCommand.normalizeShell("\(verb) x").failure, .protocolVerb(verb))
        }
        XCTAssertEqual(HvfGuestCommand.normalizeShell("\u{0B}PING 1.1.1.1").failure, .protocolVerb("PING"))
        XCTAssertTrue(HvfGuestCommandError.protocolVerb("DEL").message.contains("소문자 'del'"))
        for command in ["ping 8.8.8.8", "Del C:\\x", "echo PING", "PINGX", "RUN x", "PS x", "whoami"] {
            XCTAssertEqual(try? HvfGuestCommand.normalizeShell(command).get(), command)
        }
        XCTAssertEqual(HvfGuestCommand.normalizeShell("a\r\nb").failure, .multiline)
    }

    func testDiagnosticsNormalizationKeepsIntentionalProtocolVerbs() {
        for command in ["PING", "CLIPGET", "WINLIST", "DEL QzpceA=="] {
            XCTAssertEqual(try? HvfGuestCommand.normalize(command).get(), command)
        }
    }

    func testRunInGuestRefusesUppercaseProtocolVerbsBeforeWritingControlFile() throws {
        let fixture = try StoppingAfterAppendFixture()
        defer { fixture.remove() }
        for command in ["PING 8.8.8.8", "DEL C:\\x", "LS", "CLIPGET", "WINCLOSE 1", " GET\tx"] {
            let result = fixture.backend.runInGuest(command)
            XCTAssertEqual(result.code, -1, command)
            XCTAssertTrue(result.output.contains("소문자"), "\(command): \(result.output)")
            XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.backend.ctlFilePath), command)
        }
    }

    func testRunInGuestSendsLowercaseOrEmbeddedVerbsAsShellCommands() throws {
        let fixture = try StoppingAfterAppendFixture()
        defer { fixture.remove() }
        _ = fixture.backend.runInGuest("ping 8.8.8.8")
        XCTAssertEqual(try String(contentsOfFile: fixture.backend.ctlFilePath, encoding: .utf8), "ping 8.8.8.8\n")
        try FileManager.default.removeItem(atPath: fixture.backend.ctlFilePath)
        _ = fixture.backend.runInGuest("echo PING")
        XCTAssertEqual(try String(contentsOfFile: fixture.backend.ctlFilePath, encoding: .utf8), "echo PING\n")
    }
}

@MainActor
final class ControlModelGuestCommandsTests: XCTestCase {
    func testTerminalRefusesUppercaseProtocolVerbWithLowercaseHint() async throws {
        let fixture = try StoppingAfterAppendFixture()
        defer { fixture.remove() }
        let model = ControlModel(config: fixture.config, backend: fixture.backend, startsAutomatically: false)
        model.running = true
        model.terminalInput = "PING 8.8.8.8"

        model.runTerminalCommand()
        let deadline = Date().addingTimeInterval(5)
        while model.busy, Date() < deadline { try await Task.sleep(nanoseconds: 10_000_000) }

        XCTAssertFalse(model.busy)
        XCTAssertTrue(model.terminalLog.contains("소문자"), model.terminalLog)
        XCTAssertTrue(model.terminalLog.contains("[exit -1]"), model.terminalLog)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.backend.ctlFilePath))
    }
}

/// The VM reports running until the control file exists, so a command that
/// reaches the control channel ends promptly instead of waiting 900 s.
private struct StoppingAfterAppendFixture {
    let dir: URL
    let config: VMConfig
    let backend: HvfWindowsBackend

    init() throws {
        dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        config = VMConfig(id: "win-hvf", name: "Win HVF", displayName: "Windows Preview", backendKind: "hvf-engine",
                          bootMode: "windows-hvf", bundlePath: dir.appendingPathComponent("bundle.vmbridge").path,
                          runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                          leasesPath: "", guestName: "win-hvf", displayWidth: 1280, displayHeight: 800,
                          memMiB: 4096, cpuCount: 1)
        let ctlPath = HvfWindowsBackend(config, processIsRunning: { _ in false }).ctlFilePath
        backend = HvfWindowsBackend(config, processIsRunning: { _ in !FileManager.default.fileExists(atPath: ctlPath) })
    }

    func remove() { try? FileManager.default.removeItem(at: dir) }
}

private extension Result {
    var failure: Failure? {
        if case let .failure(error) = self { return error }
        return nil
    }
}
