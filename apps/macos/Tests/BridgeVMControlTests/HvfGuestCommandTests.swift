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
        XCTAssertTrue(HvfGuestCommandError.protocolVerb("DEL").message.contains("lowercase “del”"))
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
}

private extension Result {
    var failure: Failure? {
        if case let .failure(error) = self { return error }
        return nil
    }
}
