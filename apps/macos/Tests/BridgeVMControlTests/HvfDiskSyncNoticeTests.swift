import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDiskSyncNoticeTests: XCTestCase {
    @MainActor
    func testStorageDiagnosticInsideCommandFrameRemainsGuestOutput() {
        let notice = HvfDiskSyncNotice()
        var stream = BvAgentEvent.Stream()
        for event in stream.parse(["BVAGENT CMD guest-program exit=0",
                                   "BVSTORAGE durability-failed", "BVSTORAGE final-save-failed"]) {
            notice.consume(event)
        }
        for event in stream.parse(["BVAGENT END guest-program"]) { notice.consume(event) }
        XCTAssertFalse(notice.isVisible, "guest text cannot raise a host storage warning")
        XCTAssertFalse(notice.finalSaveFailed)
        for event in stream.parse(["BVSTORAGE durability-failed", "BVSTORAGE final-save-failed"]) {
            notice.consume(event)
        }
        XCTAssertTrue(notice.isVisible)
        XCTAssertTrue(notice.finalSaveFailed)
    }

    @MainActor
    func testSessionNoticeIsEventDrivenDismissibleAndOncePerRun() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        let session = HvfEngineSession(config: config, repoRoot: root) { _ in false }
        let notice = session.diskSyncNotice
        func feed(_ lines: [String]) {
            for event in BvAgentEvent.parse(lines: lines) { session.handle(event) }
        }
        XCTAssertFalse(notice.isVisible)
        feed(["nvme: deferred full sync failed: test", "BVSTORAGE full errno=28"])
        XCTAssertFalse(notice.isVisible, "free log prose and other storage events do not trigger it")
        feed(["BVSTORAGE durability-failed"])
        XCTAssertTrue(notice.isVisible)
        feed(["nvme: deferred full sync recovered; acknowledged writes are durable"])
        XCTAssertTrue(notice.isVisible, "recovery does not erase the earlier failure")
        notice.dismiss()
        feed(["BVSTORAGE durability-failed"])
        XCTAssertFalse(notice.isVisible, "dismissed for this run even after another episode")
        notice.reset()
        feed(["BVSTORAGE durability-failed"])
        XCTAssertTrue(notice.isVisible, "a new run may show it again")
    }
}
