import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfLegacyPasteAdmissionTests: XCTestCase, HvfLegacyPasteFixture {
    private let automatic = NSRange(location: NSNotFound, length: 0)
    private var oversized: [String] { [String(repeating: "한", count: 1_980), String(repeating: "a", count: 6_000) + "\n"] }

    func testOversizedCommittedTextIsRefusedBeforeQueueAndLaterKeyStillReplays() throws {
        for text in oversized {
            try withDisplay { view, session, root in
                XCTAssertEqual(session.sendText("가"), .acceptedForProcessing)
                let first = try lastCommand(session)
                view.insertText(text, replacementRange: automatic)
                XCTAssertEqual(session.legacyPaste.backlog, [], "A known-unreplayable commit must not be accepted")
                XCTAssertTrue(session.events.contains { $0.displayText.contains("text too long for the guest cmd.exe command line") })
                XCTAssertEqual(try lastCommand(session), first)
                let enter = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero,
                    modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
                    characters: "\r", charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 0x24))
                view.keyDown(with: enter)
                try acknowledge(first, root: root); session.poll()
                XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")), "KEY ctrl+v\nKEY enter\n")
                XCTAssertNil(session.legacyPaste.pending)
            }
        }
    }

    func testDeferredOversizedTextReturnsRefusalWithoutChangingPendingPaste() throws {
        try withSession { session, root in
            XCTAssertEqual(session.sendText("가"), .acceptedForProcessing)
            let first = try lastCommand(session)
            for text in oversized {
                XCTAssertFalse(session.sendDisplayInput(.text(text)))
                XCTAssertEqual(session.legacyPaste.backlog, [])
                XCTAssertEqual(session.legacyPaste.pending?.command, first)
                XCTAssertEqual(try lastCommand(session), first)
            }
            XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("input.ctl").path))
        }
    }

    func testDeferredLargePrintableASCIIStillUsesHID() throws {
        try withDisplay { view, session, root in
            XCTAssertEqual(session.sendText("가"), .acceptedForProcessing)
            let first = try lastCommand(session)
            let text = String(repeating: "A", count: 8_192)
            view.insertText(text, replacementRange: automatic)
            XCTAssertEqual(session.legacyPaste.backlog, [.text(text)])
            XCTAssertFalse(session.events.contains { $0.displayText.contains("refused") })
            try acknowledge(first, root: root); session.poll()
            let chunk = "KEY text-hex:" + String(repeating: "41", count: 32) + "\n"
            XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")),
                           "KEY ctrl+v\n" + String(repeating: chunk, count: 256))
            XCTAssertEqual(try lastCommand(session), first)
            XCTAssertNil(session.legacyPaste.pending)
        }
    }

    func testOversizedRefusalPreservesPreviouslyHeldValidInput() throws {
        try withSession { session, root in
            XCTAssertEqual(session.sendText("가"), .acceptedForProcessing)
            let first = try lastCommand(session)
            XCTAssertTrue(session.sendDisplayInput(.text("기")))
            XCTAssertTrue(session.sendDisplayInput(.key("left")))
            let held = session.legacyPaste.backlog
            XCTAssertFalse(session.sendDisplayInput(.text(oversized[0])))
            XCTAssertEqual(session.legacyPaste.backlog, held)
            XCTAssertEqual(session.legacyPaste.pending?.command, first)
            try acknowledge(first, root: root); session.poll()
            let second = try lastCommand(session)
            XCTAssertTrue(second.contains(Data("기".utf8).base64EncodedString()))
            try acknowledge(second, root: root); session.poll()
            XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")), "KEY ctrl+v\nKEY ctrl+v\nKEY left\n")
        }
    }

    func testLargestSingleUnicodeCommitStillReplaysAsAnotherPaste() throws {
        try withDisplay { view, session, root in
            XCTAssertEqual(session.sendText("가"), .acceptedForProcessing)
            let first = try lastCommand(session)
            let text = String(repeating: "한", count: 1_979)
            view.insertText(text, replacementRange: automatic)
            XCTAssertEqual(session.legacyPaste.backlog, [.text(text)])
            XCTAssertFalse(session.events.contains { $0.displayText.contains("refused") })
            try acknowledge(first, root: root); session.poll()
            let second = try lastCommand(session)
            XCTAssertNotEqual(second, first)
            XCTAssertEqual(second.utf8.count, HvfClipboardPaste.maximumCommandCharacters - 1)
            XCTAssertTrue(second.contains(Data(text.utf8).base64EncodedString()))
            try acknowledge(second, root: root); session.poll()
            XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("input.ctl")), "KEY ctrl+v\nKEY ctrl+v\n")
        }
    }

    private func withDisplay(_ body: (FBLayerView, HvfEngineSession, URL) throws -> Void) throws {
        try withSession { session, root in
            var frame = Data(count: 68)
            for (offset, value): (Int, UInt32) in [(0, 0x42564642), (8, 1), (12, 1), (16, 4), (24, 2)] {
                var little = value.littleEndian
                withUnsafeBytes(of: &little) { frame.replaceSubrange(offset..<(offset + 4), with: $0) }
            }
            try frame.write(to: root.appendingPathComponent("display.fb"))
            let view = FBLayerView(session: session)
            defer { view.teardown() }
            view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
            view.refreshFrame(at: 0)
            XCTAssertTrue(view.guestInputAvailable)
            try body(view, session, root)
        }
    }
}
