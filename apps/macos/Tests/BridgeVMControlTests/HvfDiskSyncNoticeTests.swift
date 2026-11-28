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
        XCTAssertEqual(HvfDiskSyncNotice.text,
            "BridgeVM couldn’t finish saving the Windows disk. Check your Mac’s free space and drive connection, then shut down Windows and start it again.")
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

    /// A drive that cannot punch holes (exFAT, HFS+): one dismissible sentence per run.
    @MainActor
    func testTrimUnsupportedLineShowsItsNoticeOncePerRun() {
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
        let line = "BVSTORAGE trim-unsupported path=/Volumes/My Passport"
        feed(["BVAGENT CMD guest-program exit=0", line, "BVAGENT END guest-program"])
        XCTAssertFalse(notice.trimUnsupportedVisible, "guest output cannot raise it")
        feed([line])
        XCTAssertTrue(notice.trimUnsupportedVisible)
        XCTAssertFalse(notice.isVisible, "not a save failure")
        XCTAssertEqual(HvfDiskSyncNotice.trimText,
            "The drive this VM is on can’t give back the space Windows frees, so the VM’s disk file won’t shrink. Move the VM to an APFS drive to save space.")
        notice.dismissTrimUnsupported()
        feed([line])
        XCTAssertFalse(notice.trimUnsupportedVisible, "once per run")
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertFalse(notice.trimUnsupportedVisible)
        feed([line])
        XCTAssertTrue(notice.trimUnsupportedVisible, "a new run says it again")
        session.resetObservedRuntimeState(clearEvents: true)
        XCTAssertFalse(notice.trimUnsupportedVisible, "a run reset clears it")
    }
}
