import Foundation
import XCTest
@testable import BridgeVMControl

final class HvfDiskSyncNoticeTests: XCTestCase {
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
