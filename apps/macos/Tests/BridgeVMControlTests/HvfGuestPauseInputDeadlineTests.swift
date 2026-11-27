import Foundation
import XCTest
@testable import BridgeVMControl

/// The app's deadlines for guest answers leave paused time out (HvfGuestRunClock).
@MainActor
final class HvfGuestPauseInputDeadlineTests: XCTestCase {
    /// Review on 8e402ec1: Pause while a pointer press still waits for its release. The release goes
    /// out before `PAUSE`, and its receipt deadline leaves the paused minute out, so the receipt that
    /// comes after Resume clears it and the next key goes through. On host time the release expired
    /// while the guest was parked, which blocked all input until the input session was reset.
    func testAReleaseSentAtPauseKeepsItsDeadlineWhileTheGuestIsPaused() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var now = Date(timeIntervalSince1970: 2_000)
            session.hostNow = { now }
            try fixture.negotiateOrderedInput(session)
            let size = CGSize(width: 100, height: 100)
            XCTAssertEqual(session.sendPointerPress(location: CGPoint(x: 10, y: 10), viewSize: size, imageSize: size), "queued")
            let press = try XCTUnwrap(try fixture.commands().last)

            XCTAssertEqual(session.requestPause(), .requested)
            let release = try XCTUnwrap(try fixture.commands().last)
            XCTAssertNotEqual(release, press, "the held button's release goes out before PAUSE")
            try fixture.appendLog("BVPAUSE paused\n")
            session.poll()
            XCTAssertEqual(session.pause, .paused)
            now += 60
            session.poll() // the session keeps polling while the guest is parked

            XCTAssertEqual(session.requestResume(), .requested)
            try fixture.appendLog("BVPAUSE resumed paused_ms=60000 vtimer_offset=0x2b\n")
            session.poll()
            XCTAssertEqual(session.pause, .running)
            let fields = release.split(separator: " ")
            XCTAssertEqual(fields.count, 3)
            guard fields.count == 3 else { return }
            let label = fields.prefix(2).joined(separator: " ")
            try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(fields[1]) 1\nBVAGENT END \(label)\n")
            session.poll()

            XCTAssertNotEqual(session.inputDriver.router.state, .failed, "input is not blocked")
            XCTAssertTrue(session.sendKey("enter"), "the next key is accepted")
            XCTAssertEqual(try fixture.commands().count, 4, "INPUTCAPS, press, release, key")
            XCTAssertEqual(try fixture.bytes(fixture.input), Data("PAUSE\nRESUME\n".utf8))
        }
    }

    /// Crash and relaunch audit, finding 5: the receipt deadlines ran on the wall clock, which keeps going while the
    /// Mac sleeps and the guest does not, so a key still awaited at sleep expired on wake and stopped all input until
    /// Windows restarted. They run on the session's uptime (ownedStartNow), which a sleep does not advance.
    func testReceiptDeadlinesCountUptimeSoAMacSleepDoesNotExpireAnAwaitedKey() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var uptime: TimeInterval = 1_000
            session.ownedStartNow = { uptime }
            try fixture.negotiateOrderedInput(session)
            let count = try XCTUnwrap(HvfGuestInputEncoding(.key("enter"))).insertedEventCount
            XCTAssertTrue(session.sendKey("enter"))
            let key = try XCTUnwrap(try fixture.commands().last)
            let words = key.split(separator: " "), label = words.prefix(2).joined(separator: " ")
            // Windows answered just before the Mac slept; the app reads it on wake. The sleep added no uptime.
            uptime += 29
            try fixture.appendLog("BVAGENT CMD \(label) exit=0\nBVINPUT_INSERTED \(words[1]) \(count)\nBVAGENT END \(label)\n")
            session.poll()
            XCTAssertNotEqual(session.inputDriver.router.state, .failed, "the key went in")
            XCTAssertFalse(session.inputStopped)

            // 30 s of uptime with no answer still expire a key and stop input.
            XCTAssertTrue(session.sendKey("tab"))
            uptime += 30
            session.poll()
            XCTAssertEqual(session.inputDriver.router.state, .failed)
            XCTAssertTrue(session.inputStopped)
        }
    }
}
