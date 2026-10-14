import AppKit
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfFailedPointerReleaseTests: XCTestCase {
    func testRejectedLaterTextDoesNotPreventReleasingConfirmedPointerPress() throws {
        try checkRelease(afterTextFailure: true)
    }

    func testHealthyTargetCancellationReleasesConfirmedPointerPress() throws {
        try checkRelease(afterTextFailure: false)
    }

    private func checkRelease(afterTextFailure failed: Bool) throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            let point = CGPoint(x: 50, y: 50), size = CGSize(width: 100, height: 100)
            XCTAssertEqual(session.sendPointerPress(location: point, viewSize: size, imageSize: size), "queued")
            let press = try XCTUnwrap(try fixture.commands().last)
            XCTAssertTrue(press.hasPrefix("POINTERINPUT "))
            try receipt(press, inserted: 1, fixture: fixture)
            session.poll()
            if failed {
                XCTAssertEqual(session.sendText("later text"), .acceptedForProcessing)
                let text = try XCTUnwrap(try fixture.commands().last)
                XCTAssertTrue(text.hasPrefix("TEXTINPUT "))
                try receipt(text, inserted: nil, fixture: fixture)
                session.poll()
                XCTAssertEqual(session.sendText("must stay refused"), .refused)
            }
            let before = try fixture.commands()
            // These are the public session paths used by mouseUp and focus/teardown.
            session.sendPointerRelease(location: point, viewSize: size, imageSize: size)
            session.cancelOrderedInputTarget()
            session.poll()
            let after = try fixture.commands()
            XCTAssertGreaterThan(after.count, before.count,
                "A confirmed held button needs cleanup even when later user input has failed")
            let cleanup = try XCTUnwrap(after.last)
            XCTAssertTrue(cleanup.hasPrefix("POINTERINPUT "), "Expected a cleanup pointer request, got \(cleanup)")
            let fields = cleanup.split(separator: " ")
            if fields.count == 3 {
                let payload = Data(base64Encoded: String(fields[2])).map { String(decoding: $0, as: UTF8.self) }
                XCTAssertTrue(payload?.hasPrefix("releaseall:") == true, "Expected releaseall, got \(payload ?? "invalid")")
            } else { XCTFail("Expected encoded cleanup command") }
            if failed { XCTAssertEqual(session.sendText("still refused"), .refused) }
            XCTAssertNil(try fixture.bytes(fixture.input))
        }
    }

    private func receipt(_ command: String, inserted: Int?, fixture: HvfRuntimeKeyboardDraftFixture) throws {
        let words = command.split(separator: " ")
        let label = words.prefix(2).joined(separator: " ")
        let body = inserted.map { "BVINPUT_INSERTED \(words[1]) \($0)\n" } ?? ""
        let text = "BVAGENT CMD \(label) exit=\(inserted == nil ? 1 : 0)\n" + body + "BVAGENT END \(label)\n"
        let file = fixture.evidence.appendingPathComponent("run.log")
        let handle = try FileHandle(forWritingTo: file)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
