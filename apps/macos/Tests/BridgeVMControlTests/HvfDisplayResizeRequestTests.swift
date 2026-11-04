import CoreGraphics
import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDisplayResizeRequestTests: XCTestCase {
    private typealias Request = HvfDisplayResizeRequest

    private func view(_ width: CGFloat, _ height: CGFloat, scale: CGFloat = 1) -> Request.View {
        .init(points: CGSize(width: width, height: height), scale: scale)
    }

    private func retina(_ width: CGFloat, _ height: CGFloat, scale: CGFloat) -> Request.Size? {
        Request.size(for: view(width, height, scale: scale), retina: true)
    }

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
        request.viewResized(to: view(1300, 800), at: 10)
        request.viewResized(to: view(1517, 931), at: 10.25)
        XCTAssertNil(request.take(at: 10.6), "the second change restarts the debounce")
        XCTAssertEqual(request.take(at: 10.66), .init(width: 1517, height: 931))
        XCTAssertNil(request.take(at: 20), "one request per settled size")
    }

    func testDuplicatesAreDroppedUntilTheDeviceRestarts() {
        var request = Request()
        let size = Request.Size(width: 1517, height: 931)
        request.viewResized(to: view(1517, 931), at: 0)
        request.settle()
        XCTAssertEqual(request.take(at: 0), size, "the end of a live resize does not wait for the debounce")
        request.didSend(size)
        request.viewResized(to: view(1517, 931), at: 1)
        request.viewResized(to: view(1600, 900), at: 1)
        request.viewResized(to: view(1517.2, 931), at: 1.1)
        request.viewResized(to: view(0, 0), at: 1.2)
        XCTAssertNil(request.take(at: 5), "back at the sent size")
        request.deviceRestarted()
        XCTAssertEqual(request.take(at: 5), size, "a restarted display device gets the size again")
    }

    func testRetinaAsksForBackingPixelsAtTheScreensScale() {
        XCTAssertEqual(retina(1280, 800, scale: 2), .init(width: 2560, height: 1600, scale: 200))
        XCTAssertEqual(retina(1280, 800, scale: 2)?.command, "RESIZE 2560x1600@200")
        XCTAssertEqual(retina(1000.5, 700, scale: 2)?.command, "RESIZE 2001x1400@200")
        XCTAssertEqual(retina(1000, 700, scale: 1.5)?.command, "RESIZE 1500x1050@150")
        XCTAssertEqual(retina(400, 300, scale: 2)?.command, "RESIZE 800x600@200")
        XCTAssertEqual(retina(300, 200, scale: 2)?.command, "RESIZE 640x480@200", "the same minimum")
        XCTAssertNil(retina(0, 0, scale: 2))
        let snapped: [(CGFloat, Int)] = [(1.1, 100), (1.25, 125), (1.3, 125), (2, 200), (2.2, 225), (2.4, 250), (3, 300), (6, 500)]
        for (factor, percent) in snapped {
            XCTAssertEqual(Request.scale(factor), percent, "scale \(factor)")
        }
        XCTAssertEqual(Request.scale(.nan), 100)
    }

    /// #493's second live run: viogpudo takes modes up to 3840x2160's pixel count once the EDID lists it.
    func testRetinaUpTo3840x2160PixelsIsFullSize() {
        XCTAssertEqual(Request.retinaMaximumPixels, 8_294_400)
        XCTAssertEqual(retina(1728, 1117, scale: 2), .init(width: 3456, height: 2234, scale: 200), "16-inch full screen")
        XCTAssertEqual(retina(1512, 982, scale: 2)?.command, "RESIZE 3024x1964@200", "14-inch full screen")
        XCTAssertEqual(retina(1920, 1080, scale: 2)?.command, "RESIZE 3840x2160@200", "exactly the limit")
    }

    func testRetinaAboveTheLimitAsksForPointsAt100() {
        XCTAssertEqual(retina(2560, 1440, scale: 2)?.command, "RESIZE 2560x1440@100", "5K full screen, 5120x2880")
        XCTAssertEqual(retina(3008, 1692, scale: 2)?.command, "RESIZE 3008x1692@100", "6K full screen, 6016x3384")
        XCTAssertEqual(retina(1920, 1080.5, scale: 2)?.command, "RESIZE 1920x1081@100", "3840x2161: one row too many")
        XCTAssertEqual(retina(2000, 1000, scale: 2)?.command, "RESIZE 2000x1000@100", "4000x2000: fewer pixels, but a side above 3840")
        XCTAssertEqual(retina(6016, 3384, scale: 2)?.command, "RESIZE 3840x2160@100", "the same 1x clamp")
    }

    /// A VM that never set the setting asks exactly as before Retina mode: points, plain, on any screen.
    func testANeverSetVMAsksExactlyAsBefore() {
        for (width, height) in [(1517, 931), (1516.6, 930.4), (320, 200), (6016, 3384)] as [(CGFloat, CGFloat)] {
            let points = Request.size(points: CGSize(width: width, height: height))
            XCTAssertNil(points?.scale)
            XCTAssertEqual(Request.size(for: view(width, height, scale: 2), retina: nil), points, "2x screen")
            XCTAssertEqual(Request.size(for: view(width, height, scale: 1), retina: nil), points, "1x screen")
        }
        XCTAssertEqual(Request.size(for: view(1517, 931, scale: 2), retina: nil)?.command, "RESIZE 1517x931")

        var request = Request()
        request.viewResized(to: view(1300, 800, scale: 2), at: 10)
        request.viewResized(to: view(1517, 931, scale: 2), at: 10.25)
        XCTAssertNil(request.take(at: 10.6), "the same debounce")
        XCTAssertEqual(request.take(at: 10.66)?.command, "RESIZE 1517x931")
    }

    /// Once set, every request names the scale: Windows keeps one across mode switches and restarts.
    func testOnceSetEveryRequestNamesTheScale() {
        XCTAssertEqual(retina(1517, 931, scale: 1)?.command, "RESIZE 1517x931@100", "on, 1x screen")
        XCTAssertEqual(retina(320, 200, scale: 1)?.command, "RESIZE 640x480@100", "on, the same clamp")
        for scale in [CGFloat(1), 2] {
            XCTAssertEqual(Request.size(for: view(1517, 931, scale: scale), retina: false)?.command, "RESIZE 1517x931@100",
                           "off after on, scale \(scale)")
        }
        XCTAssertEqual(Request.size(for: view(6016, 3384, scale: 2), retina: false)?.command, "RESIZE 3840x2160@100")
        XCTAssertNil(Request.size(for: view(0, 0, scale: 2), retina: false))

        var request = Request()
        request.retina = false
        request.viewResized(to: view(1300, 800, scale: 2), at: 10)
        request.viewResized(to: view(1517, 931, scale: 2), at: 10.25)
        XCTAssertNil(request.take(at: 10.6), "the same debounce")
        XCTAssertEqual(request.take(at: 10.66)?.command, "RESIZE 1517x931@100")
    }

    func testMovingBetweenScreensSendsANewRequestOnlyInPixels() {
        for setting in [nil, false] as [Bool?] {
            var request = Request()
            request.retina = setting
            request.viewResized(to: view(1280, 800, scale: 1), at: 0)
            let points = request.take(at: 1)
            XCTAssertEqual(points?.command, setting == nil ? "RESIZE 1280x800" : "RESIZE 1280x800@100")
            request.didSend(points!)
            request.viewResized(to: view(1280, 800, scale: 2), at: 2)
            XCTAssertNil(request.take(at: 3), "in points the request is unchanged")
        }

        var request = Request()
        request.retina = true
        var sent: [String] = []
        for (scale, at) in [(CGFloat(1), 0.0), (2, 2), (2, 3), (1, 4), (1, 5), (2, 6)] {
            request.viewResized(to: view(1280, 800, scale: scale), at: at)
            XCTAssertNil(request.take(at: at + 0.3), "debounced")
            if let size = request.take(at: at + 0.5) {
                request.didSend(size)
                sent.append(size.command)
            }
        }
        // The first request already names 100 %: an earlier run may have left Windows at 200 %.
        XCTAssertEqual(sent, ["RESIZE 1280x800@100", "RESIZE 2560x1600@200", "RESIZE 1280x800@100", "RESIZE 2560x1600@200"])
        request.deviceRestarted()
        XCTAssertEqual(request.take(at: 7)?.command, "RESIZE 2560x1600@200", "a restarted device gets it again")
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

    /// A session asks in backing pixels only with Retina resolution on and the 2D virtio display; once the
    /// setting was saved, its every request names the scale, and a never-set VM's request is plain.
    @MainActor
    func testTheSessionsRequestFollowsTheSavedSetting() throws {
        let cases: [(retina: Bool?, virtio2d: Bool, virtio3d: Bool, line: String)] = [
            (nil, true, false, "RESIZE 1280x800"), (true, true, false, "RESIZE 2560x1600@200"),
            (false, true, false, "RESIZE 1280x800@100"), (true, false, false, "RESIZE 1280x800@100"),
            (true, true, true, "RESIZE 1280x800@100")]
        for (retina, virtio2d, virtio3d, line) in cases {
            let fixture = try HvfRuntimeKeyboardDraftFixture()
            defer { fixture.remove() }
            try fixture.withSession { session in
                // Set after the agent starts: a 2D virtio-gpu session also sends the hardware cursor
                // command then (HvfDisplayDriverHardwareCursor.swift), and the fixture expects none.
                try fixture.negotiateOrderedInput(session)
                session.config.retinaResolution = retina
                session.config.virtioGpu2d = virtio2d
                session.config.virtioGpu3d = virtio3d
                session.displayViewResized(to: CGSize(width: 1280, height: 800), scale: 2, settled: true)
                XCTAssertEqual(try fixture.bytes(fixture.input), Data((line + "\n").utf8), line)
            }
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
