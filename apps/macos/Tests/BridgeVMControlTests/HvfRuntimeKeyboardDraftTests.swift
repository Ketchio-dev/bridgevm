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

                XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)

                XCTAssertEqual(draft, original, "Refused text must remain editable: \(state)")
                XCTAssertEqual(session.connectionState, state)
                XCTAssertEqual(session.events.last, .unknown("clipboard paste refused: guest service is not connected"))
                XCTAssertNil(try fixture.bytes(fixture.control))
                XCTAssertNil(try fixture.bytes(fixture.input))
            }
        }
    }

    func testInterruptedClipboardPasteKeepsDraftAndNeverSends() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.connect(session)
            var first = "첫 번째 문장"
            let original = first
            let encoded = Data(first.utf8).base64EncodedString()
            var sent = false
            XCTAssertEqual(HvfKeyboardDraft.submit(&first, to: session) { sent = true }, .acceptedForProcessing)
            XCTAssertEqual(first, original)
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
                XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)
                XCTAssertEqual(draft, text)
                XCTAssertEqual(session.events.last, .unknown("text input refused: clipboard paste pending"))
                XCTAssertEqual(try fixture.bytes(fixture.control), controlBefore)
                XCTAssertNil(try fixture.bytes(fixture.input))
            }
            let marker = try XCTUnwrap(session.legacyPaste.pending?.marker)
            session.cancelOrderedInputTarget()
            try fixture.appendLog("BVAGENT CMD \(marker) exit=0\n\(marker)\nBVAGENT END \(marker)\n")
            session.poll()
            XCTAssertEqual(first, original)
            XCTAssertFalse(sent)
            XCTAssertNil(try fixture.bytes(fixture.input))
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

            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)

            XCTAssertEqual(draft, original)
            XCTAssertEqual(session.events.last, .unknown("ordered input refused: unavailable, full or changed ownership"))
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertFalse(FileManager.default.fileExists(atPath: changed.path))
        }
    }

    func testDeclinedMultilineDraftPastesOnceBeforeFollowingKey() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session, text: ["QUEUED_DISPATCH"])
            let original = "첫째 줄\r\n둘째 줄 🙂"
            var draft = original
            var sent = 0
            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session) { sent += 1 }, .acceptedForProcessing)
            XCTAssertEqual(draft, original)
            let command = try XCTUnwrap(fixture.commands().last)
            let words = command.split(separator: " ")
            XCTAssertEqual(words.first, "TEXTINPUT")
            XCTAssertEqual(words.last, "S")
            let id = try XCTUnwrap(words.dropFirst().first)
            XCTAssertTrue(session.sendKey("enter"))
            XCTAssertEqual(try fixture.commands().count, 2)

            let label = "TEXTINPUT \(id)"
            try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVTEXT_UNSUPPORTED \(id)\nBVAGENT END \(label)\n")
            session.poll()
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, 3)
            let request = try XCTUnwrap(session.legacyPaste.pending)
            XCTAssertEqual(commands.last, request.command)
            XCTAssertTrue(request.command.hasPrefix("CLIPPASTE "))
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertEqual(sent, 0)

            let receipt = "BVAGENT CMD \(request.marker) exit=0\n\(request.marker)\nBVAGENT END \(request.marker)\n"
            try fixture.appendLog(receipt + receipt)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY ctrl+v\nKEY enter\n".utf8))
            XCTAssertEqual(sent, 1)
        }
    }

    func testMultilineDraftUsesOnePasteWhenAgentLacksQueuedDispatch() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            XCTAssertTrue(session.sendKey("enter"))
            let beforeDraft = try fixture.commands()
            let earlierKey = try XCTUnwrap(beforeDraft.last)
            let id = try XCTUnwrap(earlierKey.split(separator: " ").dropFirst().first)
            var draft = "첫 줄\r\n둘째 줄 🙂"
            let base64 = Data(draft.utf8).base64EncodedString()
            var sent = 0
            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session) { sent += 1 }, .acceptedForProcessing)
            XCTAssertTrue(session.sendKey("tab"))
            XCTAssertEqual(try fixture.commands(), beforeDraft)

            let label = "KEYINPUT \(id)"
            try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(id) 2\nBVAGENT END \(label)\n")
            session.poll()
            let request = try XCTUnwrap(session.legacyPaste.pending)
            XCTAssertTrue(request.command.hasPrefix("CLIPPASTE "))
            XCTAssertTrue(request.command.hasSuffix(" " + base64))
            XCTAssertEqual(try fixture.commands(), beforeDraft + [request.command])
            XCTAssertNil(try fixture.bytes(fixture.input))
            XCTAssertEqual(sent, 0)

            let receipt = "BVAGENT CMD \(request.marker) exit=0\n\(request.marker)\nBVAGENT END \(request.marker)\n"
            try fixture.appendLog(receipt + receipt)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("KEY ctrl+v\nKEY tab\n".utf8))
            XCTAssertEqual(sent, 1)
        }
    }

    func testFullOrderedQueuePreservesRefusedDraftAndExistingCommandBytes() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            for _ in 0..<HvfOrderedInputQueue.maximumEvents {
                XCTAssertEqual(session.sendText("a"), .acceptedForProcessing)
            }
            let commands = try fixture.commands()
            XCTAssertEqual(commands.count, 2, "Only the first ordered text is sent without a completion receipt")
            XCTAssertTrue(try XCTUnwrap(commands.last).hasPrefix("TEXTINPUT "))
            let controlBefore = try fixture.bytes(fixture.control)
            var draft = "대기열이 가득 차도 남길 문장"
            let original = draft

            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)

            XCTAssertEqual(draft, original)
            XCTAssertEqual(session.events.last, .unknown("ordered input refused: unavailable, full or changed ownership"))
            XCTAssertEqual(try fixture.bytes(fixture.control), controlBefore)
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testTargetChangeKeepsAnUnsentOrderedDraft() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            XCTAssertTrue(session.sendKey("enter"))
            let beforeDraft = try fixture.commands()
            let keyRequest = try XCTUnwrap(beforeDraft.last)
            XCTAssertTrue(keyRequest.hasPrefix("KEYINPUT "))
            let keyID = try XCTUnwrap(keyRequest.split(separator: " ").dropFirst().first)
            let original = "순서대로 입력 🙂"
            var draft = original
            var completions = 0

            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session) { completions += 1 }, .acceptedForProcessing)
            XCTAssertEqual(draft, original)
            XCTAssertEqual(try fixture.commands(), beforeDraft, "Text waits behind the earlier key")

            session.cancelOrderedInputTarget()
            let label = "KEYINPUT \(keyID)"
            try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(keyID) 2\nBVAGENT END \(label)\n")
            session.poll()

            XCTAssertEqual(draft, original)
            XCTAssertEqual(completions, 0)
            XCTAssertEqual(try fixture.commands(), beforeDraft, "The old target's late receipt cannot send the draft")
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testEmptyAndASCIIDraftsPreserveLegacyClearBehavior() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var draft = ""
            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)
            XCTAssertEqual(draft, "")
            XCTAssertTrue(session.events.isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.control))
            XCTAssertNil(try fixture.bytes(fixture.input))

            draft = "hello"
            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .legacyAttempted)
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

            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)

            XCTAssertEqual(draft, original)
            XCTAssertTrue(session.events.last?.displayText.contains("control command write failed:") == true)
            XCTAssertEqual(try fixture.control.resourceValues(forKeys: [.isDirectoryKey]).isDirectory, true)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.control.path).isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    func testFailedLegacyASCIIWriteRetainsDraftWithoutReplacingDirectory() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try FileManager.default.createDirectory(at: fixture.input, withIntermediateDirectories: false)
            var draft = "legacy ASCII"

            XCTAssertEqual(HvfKeyboardDraft.submit(&draft, to: session), .refused)

            XCTAssertEqual(draft, "legacy ASCII", "Refused text must remain editable")
            XCTAssertTrue(session.events.last?.displayText.contains("live input write failed:") == true)
            XCTAssertEqual(try fixture.input.resourceValues(forKeys: [.isDirectoryKey]).isDirectory, true)
            XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: fixture.input.path).isEmpty)
            XCTAssertNil(try fixture.bytes(fixture.control))
        }
    }
}
