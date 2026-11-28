import Foundation
import XCTest
@testable import BridgeVMControl

/// The VM paused only once a write found the drive with its disk full; nothing warned before. The
/// session's poll now reads that drive's free space while Windows runs and says so in one sentence.
@MainActor
final class HvfLowSpaceNoticeTests: XCTestCase {
    /// Below the threshold the notice shows; the poll reads at most every 30 s and only while the VM
    /// runs; space coming back clears it, and it does not come back this run, nor once dismissed.
    func testAPollWithLittleSpaceWarnsOnceAndSpaceComingBackClearsIt() throws {
        let fixture = try HvfRuntimeKeyboardDraftFixture()
        defer { fixture.remove() }
        try fixture.withSession { session in
            var now: TimeInterval = 100
            var free = HvfLowSpaceNotice.threshold - 1
            var reads = 0
            session.ownedStartNow = { now }
            session.freeSpace = { _ in reads += 1; return free }
            session.poll()
            XCTAssertEqual(reads, 0, "a stopped VM is not checked")

            session.connectionState = .booting
            session.poll()
            let text = try XCTUnwrap(session.lowSpaceNotice.text)
            XCTAssertTrue(text.hasPrefix("Only "), text)
            // The fixture's disk is on no drive the host can name: the Mac's own wording.
            XCTAssertTrue(text.hasSuffix(" is left on your Mac. Windows pauses if it fills up, so free up space or delete snapshots."), text)
            now += 29
            session.poll()
            XCTAssertEqual(reads, 1, "at most every 30 s")

            now += 1
            free = HvfLowSpaceNotice.threshold
            session.poll()
            XCTAssertEqual(reads, 2)
            XCTAssertNil(session.lowSpaceNotice.text, "space came back")
            now += 30
            free = 1
            session.poll()
            XCTAssertNil(session.lowSpaceNotice.text, "once per run")

            session.resetObservedRuntimeState(clearEvents: false) // a new run
            session.connectionState = .booting
            session.poll()
            XCTAssertNotNil(session.lowSpaceNotice.text, "a new run warns again")
            session.lowSpaceNotice.dismiss()
            now += 30
            session.poll()
            XCTAssertNil(session.lowSpaceNotice.text, "dismissed for this run")
        }
    }

    /// A VM on another drive names it; Foundation gives the startup disk's mount point as `/`.
    func testTheNoticeNamesADriveOtherThanTheMacsStartupDisk() {
        XCTAssertTrue(HvfLowSpaceNotice.text(free: 1, drive: "Mac SSD").contains(" is left on “Mac SSD”. Windows pauses"))
        XCTAssertNil(HvfVMDrive.name(mountPoint: "/"))
        XCTAssertEqual(HvfVMDrive.name(mountPoint: "/Volumes/Mac SSD"), "Mac SSD")
    }
}
