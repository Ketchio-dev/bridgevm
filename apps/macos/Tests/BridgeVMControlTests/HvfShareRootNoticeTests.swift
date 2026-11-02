import Combine
import Foundation
import XCTest
@testable import BridgeVMControl

/// The share-root warning follows the runtime's exact raw notices; it is informational only.
final class HvfShareRootNoticeTests: XCTestCase {
    private let blocked = "BVAGENT SHARE root-state blocked"
    private let ready = "BVAGENT SHARE root-state ready"

    @MainActor
    private func session() -> HvfEngineSession {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let config = HvfEngineConfig(targetDiskPath: root.appendingPathComponent("disk").path,
            uefiVarsPath: root.appendingPathComponent("vars").path, evidenceDir: root.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: false, shareHostDir: nil, shareGuestDir: nil,
            virtioNet: true, virtioGpu3d: false, nvmeBufferedIO: true,
            ctlFilePath: root.appendingPathComponent("agent.ctl").path)
        return HvfEngineSession(config: config, repoRoot: root) { _ in false }
    }

    @MainActor
    private func feed(_ session: HvfEngineSession, _ lines: [String]) {
        for event in BvAgentEvent.parse(lines: lines) { session.handle(event) }
    }

    @MainActor
    func testExactMarkersWithOptionalCarriageReturnChangeStateAndVariantsDoNot() {
        let notice = HvfShareRootNotice()
        for variant in [blocked + " ", " " + blocked, blocked + "\r\r", "BVAGENT SHARE root-state BLOCKED",
                        blocked + " t=1", "BVAGENT SHARE root-state", "BVAGENT  SHARE root-state blocked"] {
            notice.consume(.unknown(variant))
            XCTAssertFalse(notice.isBlocked, variant)
        }
        notice.consume(.unknown(blocked + "\r"))
        XCTAssertTrue(notice.isBlocked)
        for variant in [ready + " ", ready + "\r\r", "BVAGENT SHARE root-state READY", ready + " t=2"] {
            notice.consume(.unknown(variant))
            XCTAssertTrue(notice.isBlocked, variant)
        }
        notice.consume(.unknown(ready))
        XCTAssertFalse(notice.isBlocked)
        notice.consume(.unknown(blocked))
        XCTAssertTrue(notice.isBlocked)
        notice.consume(.unknown(ready + "\r"))
        XCTAssertFalse(notice.isBlocked)
    }

    @MainActor
    func testMarkersInsideCommandOutputFramesLeaveStateUnchanged() {
        let session = session()
        feed(session, ["BVAGENT CMD type notes.txt exit=0", blocked, "BVAGENT END type notes.txt"])
        XCTAssertFalse(session.shareRootNotice.isBlocked)
        feed(session, [blocked])
        XCTAssertTrue(session.shareRootNotice.isBlocked)
        feed(session, ["BVAGENT CMD type notes.txt exit=0", ready, "BVAGENT END type notes.txt"])
        XCTAssertTrue(session.shareRootNotice.isBlocked)
    }

    @MainActor
    func testGuestResetRecordClearsTheNoticeButCommandOutputTextDoesNot() {
        let session = session()
        feed(session, [blocked])
        feed(session, ["BVAGENT CMD echo exit=0", HvfGuestResetRecord.processRecreation, "BVAGENT END echo"])
        XCTAssertTrue(session.shareRootNotice.isBlocked)
        feed(session, [HvfGuestResetRecord.processRecreation + "\r"])
        XCTAssertFalse(session.shareRootNotice.isBlocked)
        feed(session, [blocked, "PSCI SYSTEM_RESET: reboot 1/8"])
        XCTAssertFalse(session.shareRootNotice.isBlocked)
        feed(session, [blocked, "PSCI SYSTEM_RESET: reboot one/8"])
        XCTAssertTrue(session.shareRootNotice.isBlocked)
    }

    @MainActor
    func testPublicationHappensOnlyForActualStateChanges() {
        let notice = HvfShareRootNotice()
        var published: [Bool] = []
        var willChange = 0
        let values = notice.$isBlocked.dropFirst().sink { published.append($0) }
        let changes = notice.objectWillChange.sink { willChange += 1 }
        defer { values.cancel(); changes.cancel() }
        notice.reset()
        notice.consume(.unknown(ready))
        notice.consume(.unknown(blocked))
        notice.consume(.unknown(blocked + "\r"))
        notice.consume(.unknown("unrelated"))
        notice.consume(.unknown(ready))
        notice.consume(.unknown(ready))
        notice.consume(.unknown(blocked))
        notice.reset()
        notice.reset()
        XCTAssertEqual(published, [true, false, true, false])
        XCTAssertEqual(willChange, 4)
    }

    @MainActor
    func testTheNoticeOutlivesEvictionOfItsMarkerFromTheEventRing() {
        let session = session()
        session.append(.unknown(blocked))
        for index in 0..<600 { session.append(.unknown("noise \(index)")) }
        XCTAssertEqual(session.events.count, 500)
        XCTAssertFalse(session.events.contains(.unknown(blocked)))
        XCTAssertTrue(session.shareRootNotice.isBlocked)
    }

    @MainActor
    func testNewStartsResetAndMarkStoppedResetsOnlyWhenItsGuardAdmits() {
        let session = session()
        for clearEvents in [false, true] {
            session.append(.unknown(blocked))
            XCTAssertTrue(session.shareRootNotice.isBlocked)
            session.resetObservedRuntimeState(clearEvents: clearEvents)
            XCTAssertFalse(session.shareRootNotice.isBlocked, "clearEvents=\(clearEvents)")
        }
        session.append(.unknown(blocked))
        session.runtimeTransitionInProgress = true
        session.markStopped()
        XCTAssertTrue(session.shareRootNotice.isBlocked)
        session.runtimeTransitionInProgress = false
        session.markStopped()
        XCTAssertFalse(session.shareRootNotice.isBlocked)
        XCTAssertEqual(session.connectionState, .stopped)
    }
}
