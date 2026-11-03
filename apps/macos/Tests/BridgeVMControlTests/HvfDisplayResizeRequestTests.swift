import CoreGraphics
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDisplayResizeRequestTests: XCTestCase {
    private typealias Request = HvfDisplayResizeRequest

    func testSizeIsViewPointsClampedWithoutRounding() {
        XCTAssertEqual(Request.size(points: CGSize(width: 1517, height: 931)), .init(width: 1517, height: 931))
        XCTAssertEqual(Request.size(points: CGSize(width: 1516.6, height: 930.4)), .init(width: 1517, height: 930))
        XCTAssertEqual(Request.size(points: CGSize(width: 640, height: 400)), .init(width: 640, height: 480))
        XCTAssertEqual(Request.size(points: CGSize(width: 320, height: 200)), Request.minimum)
        XCTAssertEqual(Request.size(points: CGSize(width: 6016, height: 3384)), Request.maximum)
        XCTAssertNil(Request.size(points: .zero), "a view that is not laid out asks for nothing")
        XCTAssertEqual(Request.Size(width: 1517, height: 931).command, "RESIZE 1517x931")
    }

    func testTrailingDebounceSendsOnlyTheSettledSize() {
        var request = Request()
        request.viewResized(to: CGSize(width: 1300, height: 800), at: 10)
        request.viewResized(to: CGSize(width: 1517, height: 931), at: 10.25)
        XCTAssertNil(request.take(at: 10.6), "the second change restarts the debounce")
        XCTAssertEqual(request.take(at: 10.66), .init(width: 1517, height: 931))
        XCTAssertNil(request.take(at: 20), "one request per settled size")
    }

    func testDuplicatesAreDroppedUntilTheDeviceRestarts() {
        var request = Request()
        let size = Request.Size(width: 1517, height: 931)
        request.viewResized(to: CGSize(width: 1517, height: 931), at: 0)
        request.settle()
        XCTAssertEqual(request.take(at: 0), size, "the end of a live resize does not wait for the debounce")
        request.didSend(size)
        request.viewResized(to: CGSize(width: 1517, height: 931), at: 1)
        request.viewResized(to: CGSize(width: 1600, height: 900), at: 1)
        request.viewResized(to: CGSize(width: 1517.2, height: 931), at: 1.1)
        XCTAssertNil(request.take(at: 5), "back at the sent size")
        request.deviceRestarted()
        XCTAssertEqual(request.take(at: 5), size, "a restarted display device gets the size again")
    }

    @MainActor
    func testResizeIsWrittenWhileOrderedInputIsActiveAndLeavesItUnchanged() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            let before = session.inputDriver.router
            XCTAssertTrue(before.activated)
            let commands = try fixture.commands()

            session.displayViewResized(to: CGSize(width: 1517, height: 931), settled: true)

            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESIZE 1517x931\n".utf8))
            let after = session.inputDriver.router
            XCTAssertEqual(after.state, before.state)
            XCTAssertEqual([after.activated, after.eligible, after.failed], [before.activated, before.eligible, before.failed])
            XCTAssertEqual(after.count, before.count)
            XCTAssertEqual(try fixture.commands(), commands)
            XCTAssertTrue(session.sendKey("enter"))
            XCTAssertTrue(try fixture.commands().last?.hasPrefix("KEYINPUT ") == true, "keys still use the ordered stream")
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESIZE 1517x931\n".utf8))
        }
    }

    @MainActor
    func testWindowSizeIsSentAgainAfterGuestServiceStart() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            try fixture.negotiateOrderedInput(session)
            session.displayViewResized(to: CGSize(width: 1517, height: 931), settled: true)
            session.displayViewResized(to: CGSize(width: 1517, height: 931), settled: true)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESIZE 1517x931\n".utf8))

            // Synthetic guest reboot: the recreated helper reads input.ctl from its end.
            try appendLog(HvfGuestResetRecord.processRecreation + "\nBVAGENT READY host=draft-fixture t=3\n", fixture)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESIZE 1517x931\n".utf8))
            try appendLog("BVAGENT SERVICE start t=4\n", fixture)
            session.poll()
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("RESIZE 1517x931\nRESIZE 1517x931\n".utf8))
        }
    }

    @MainActor
    private func appendLog(_ text: String, _ fixture: HvfRuntimeKeyboardDraftFixture) throws {
        let handle = try FileHandle(forWritingTo: fixture.evidence.appendingPathComponent("run.log"))
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }
}
