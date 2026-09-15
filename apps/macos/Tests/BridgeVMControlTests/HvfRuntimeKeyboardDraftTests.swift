import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeKeyboardDraftTests: XCTestCase {
    func testDisconnectedUnicodeRefusalRetainsDraftWithoutWritingInput() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            for state in [HvfConnectionState.stopped, .booting, .timedOut] {
                session.connectionState = state
                var draft = "다시 입력할 문장 🙂"
                let original = draft

                HvfKeyboardDraft.submit(&draft, to: session)

                XCTAssertEqual(draft, original, "Refused text must remain editable: \(state)")
                XCTAssertEqual(session.connectionState, state)
                XCTAssertEqual(session.events.last, .unknown("clipboard paste refused: guest service is not connected"))
                XCTAssertNil(try fixture.bytes(fixture.control))
                XCTAssertNil(try fixture.bytes(fixture.input))
            }
        }
    }

    func testClipboardAcceptanceClearsButPendingPasteRefusalKeepsLaterDrafts() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.connect(session)
            var first = "첫 번째 문장"
            let encoded = Data(first.utf8).base64EncodedString()
            HvfKeyboardDraft.submit(&first, to: session)
            XCTAssertEqual(first, "")
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, 1)
            let command = try XCTUnwrap(commands.first)
            XCTAssertTrue(command.contains("Set-Clipboard"))
            XCTAssertTrue(command.contains("FromBase64String('\(encoded)')"))
            XCTAssertTrue(command.contains("BVPASTE_READY "))
            let controlBefore = try fixture.bytes(fixture.control)
            XCTAssertNil(try fixture.bytes(fixture.input), "Host clipboard acceptance does not imply a paste")

            for text in ["다음 문장", "later ASCII"] {
                var draft = text
                HvfKeyboardDraft.submit(&draft, to: session)
                XCTAssertEqual(draft, text)
                XCTAssertEqual(session.events.last, .unknown("text input refused: clipboard paste pending"))
                XCTAssertEqual(try fixture.bytes(fixture.control), controlBefore)
                XCTAssertNil(try fixture.bytes(fixture.input))
            }
        }
    }

    func testChangedOrderedOwnershipRefusesWithoutWritingToEitherTarget() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            session.beginOwnedInputBoot()
            let changed = fixture.root.appendingPathComponent("changed-input")
            session.config.evidenceDir = changed.path
            var draft = "소유권 변경 뒤 남길 문장"
            let original = draft

            HvfKeyboardDraft.submit(&draft, to: session)

            XCTAssertEqual(draft, original)
            XCTAssertEqual(session.events.last, .unknown("ordered input refused: unavailable, full or changed ownership"))
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertFalse(FileManager.default.fileExists(atPath: changed.path))
        }
    }

    func testFullOrderedQueuePreservesRefusedDraftAndExistingCommandBytes() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            for _ in 0..<HvfOrderedInputQueue.maximumEvents {
                var accepted = "a"
                HvfKeyboardDraft.submit(&accepted, to: session)
                XCTAssertEqual(accepted, "")
            }
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, 2, "Only the first ordered text is sent without a completion receipt")
            XCTAssertTrue(try XCTUnwrap(commands.last).hasPrefix("TEXTINPUT "))
            let controlBefore = try fixture.bytes(fixture.control)
            var draft = "대기열이 가득 차도 남길 문장"
            let original = draft

            HvfKeyboardDraft.submit(&draft, to: session)

            XCTAssertEqual(draft, original)
            XCTAssertEqual(session.events.last, .unknown("ordered input refused: unavailable, full or changed ownership"))
            XCTAssertEqual(try fixture.bytes(fixture.control), controlBefore)
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testAcceptedOrderedUnicodeClearsDraftAtHostAdmission() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            var draft = "순서대로 입력 🙂"
            let encoded = Data(draft.utf8).base64EncodedString()

            HvfKeyboardDraft.submit(&draft, to: session)

            XCTAssertEqual(draft, "")
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, 2)
            let words = try XCTUnwrap(commands.last).split(separator: " ")
            XCTAssertEqual(words.count, 3)
            XCTAssertEqual(words.first, "TEXTINPUT")
            XCTAssertEqual(words.last, Substring(encoded))
            XCTAssertNil(try fixture.bytes(fixture.input))
            // No insertion or application-consumption receipt is synthesized.
        }
    }

    func testEmptyAndASCIIDraftsPreserveLegacyClearBehavior() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var draft = ""
            HvfKeyboardDraft.submit(&draft, to: session)
            XCTAssertEqual(draft, "")
            XCTAssertTrue(session.events.isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertNil(try fixture.bytes(fixture.input))

            draft = "hello"
            HvfKeyboardDraft.submit(&draft, to: session)
            XCTAssertEqual(draft, "")
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY text-hex:68656c6c6f\n".utf8))
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertEqual(session.connectionState, .stopped)
        }
    }

    func testClipboardControlWriteFailureRetainsDraftAndOwnedDirectory() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.connect(session)
            try FileManager.default.createDirectory(at: fixture.control, withIntermediateDirectories: true)
            var draft = "기록하지 못한 문장"
            let original = draft

            HvfKeyboardDraft.submit(&draft, to: session)

            XCTAssertEqual(draft, original)
            XCTAssertTrue(session.events.last?.displayText.contains("control command write failed:") == true)
            XCTAssertEqual(try fixture.control.resourceValues(forKeys: [.isDirectoryKey]).isDirectory, true)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.control.path).isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testFailedLegacyASCIIWriteStillClearsDraftWithoutReplacingDirectory() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try FileManager.default.createDirectory(at: fixture.input, withIntermediateDirectories: false)
            var draft = "legacy ASCII"

            HvfKeyboardDraft.submit(&draft, to: session)

            XCTAssertEqual(draft, "", "Legacy clearing does not guarantee a successful write")
            XCTAssertTrue(session.events.last?.displayText.contains("live input write failed:") == true)
            XCTAssertEqual(try fixture.input.resourceValues(forKeys: [.isDirectoryKey]).isDirectory, true)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.input.path).isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }
}
