#if canImport(AppKit)
import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfLegacyPasteWriteFailureTests: XCTestCase, HvfLegacyPasteFixture {
    func testReadyMarkerWithRefusedCtrlVDropsHeldInputWithoutStartingNextPaste() throws {
        try withSession { session, root in
            let input = root.appendingPathComponent("input.ctl")
            let retired = root.appendingPathComponent("retired-input")
            session.sendKey("esc") // Pin the original channel identity.
            try FileManager.default.moveItem(at: input, to: retired)
            let retiredBytes = try Data(contentsOf: retired)
            let replacement = Data("replacement".utf8)
            try replacement.write(to: input)

            XCTAssertEqual(session.sendText("한"), .acceptedForProcessing)
            let first = try lastCommand(session)
            XCTAssertTrue(session.sendDisplayInput(.key("enter")))
            XCTAssertTrue(session.sendDisplayInput(.text("글")))
            XCTAssertTrue(session.sendDisplayInput(.key("backspace")))
            XCTAssertEqual(session.legacyPaste.backlog.count, 3)

            try acknowledge(first, root: root)
            session.poll()
            XCTAssertNil(session.legacyPaste.pending)
            XCTAssertEqual(session.legacyPaste.backlog, [])
            XCTAssertEqual(try Data(contentsOf: retired), retiredBytes)
            XCTAssertEqual(try Data(contentsOf: input), replacement)
            XCTAssertEqual(try String(contentsOfFile: session.config.ctlFilePath, encoding: .utf8)
                .split(separator: "\n").map(String.init), [first], "No later Set-Clipboard may start")
            XCTAssertTrue(session.events.contains(.unknown("display input replay stopped; discarded 3 held display input entries")))
            XCTAssertTrue(session.events.contains { $0.displayText.contains("live input write failed:") })
        }
    }
}
#endif
