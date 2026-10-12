#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

/// bvagent.ps1 runs a legacy paste as `& cmd.exe /c $cmd`; Windows PowerShell quotes the
/// argument, so cmd.exe sees `"C:\Windows\system32\cmd.exe" /c "<command>"` and refuses a
/// command line over 8,191 characters. The host must refuse before clearing the draft.
@MainActor
final class HvfClipboardPasteCommandLimitTests: XCTestCase {
    private func cmdLine(_ command: String) -> String { #""C:\Windows\system32\cmd.exe" /c ""# + command + "\"" }

    func testOverLimitLegacyTextIsRefusedKeepingDraftWithoutWriting() throws {
        // 1,980 syllables = 5,940 UTF-8 bytes -> 8,159-character command; 5,938 bytes is the first refused size.
        for text in [String(repeating: "한", count: 1_980), "é" + String(repeating: "a", count: 5_936)] {
            let fixture = try HvfRuntimeKeyboardDraftFixture()
            defer { fixture.remove() }
            try fixture.withSession { session in
                try fixture.connect(session)
                var draft = text
                XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)
                XCTAssertTrue(draft == text, "Refused text must remain editable (\(draft.utf8.count) bytes left)")
                XCTAssertEqual(session.events.last, .unknown("clipboard paste refused: text too long for the guest "
                    + "cmd.exe command line (8159/8156 characters); send shorter text"))
                XCTAssertNil(try fixture.bytes(fixture.control))
                XCTAssertNil(try fixture.bytes(fixture.input))
                var next = "짧은 문장"
                XCTAssertEqual(HvfKeyboardDraft.submit(&next, to: session), .acceptedForProcessing, "No paste left pending")
                XCTAssertEqual(try fixture.commands().count, 1)
            }
        }
    }

    func testLargestAdmittedLegacyTextBuildsOneCommandWithinCmdLimit() throws {
        // 1,979 syllables and 5,937 bytes are the largest texts whose cmd.exe line fits.
        for text in [String(repeating: "한", count: 1_979), "é" + String(repeating: "a", count: 5_935)] {
            let fixture = try HvfRuntimeKeyboardDraftFixture()
            defer { fixture.remove() }
            try fixture.withSession { session in
                try fixture.connect(session)
                var draft = text
                XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .acceptedForProcessing)
                XCTAssertEqual(draft, "")
                let commands = try fixture.commands()
                XCTAssertEqual(commands.count, 1)
                let command = try XCTUnwrap(commands.first)
                XCTAssertTrue(command.contains("FromBase64String('\(Data(text.utf8).base64EncodedString())')"))
                XCTAssertEqual(command.utf16.count, 8_155)
                XCTAssertLessThan(cmdLine(command).utf16.count, 8_192)
                XCTAssertNil(try fixture.bytes(fixture.input))
            }
        }
    }

    func testDisplayKeyEventOverLimitIsConsumedButLogsRefusalWithoutWriting() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.connect(session)
            let text = String(repeating: "한", count: 1_980)
            let event = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [],
                timestamp: 0, windowNumber: 0, context: nil, characters: text,
                charactersIgnoringModifiers: text, isARepeat: false, keyCode: 0))
            // The display ignores sendText's result (existing behaviour); the reason is only logged.
            XCTAssertTrue(HvfFramebufferKeyInput.send(event, to: session))
            XCTAssertTrue(session.events.last?.displayText.hasPrefix("clipboard paste refused:") == true)
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }
}
#endif
