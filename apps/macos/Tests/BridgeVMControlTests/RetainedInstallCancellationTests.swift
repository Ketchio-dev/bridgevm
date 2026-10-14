import AppKit
import SwiftUI
import XCTest
@testable import BridgeVMControl

@MainActor
final class RetainedInstallCancellationTests: XCTestCase {
    func testRetainedFinalizationDisablesCancellationLikeOrdinaryControls() async throws {
        let fixture = try RetainedInstallCancellationFixture(finalizing: true)
        defer { fixture.clean() }
        try await fixture.run { session, host in
            XCTAssertEqual(session.stage, .finalizing)
            XCTAssertTrue(session.isRunning)
            XCTAssertFalse(session.canCancel)
            XCTAssertEqual(fixture.buttons.entries.count, 1)
            let retained = try XCTUnwrap(fixture.buttons.entries.values.first)
            let ordinaryButtons = InstallCancellationButtonProbe()
            let ordinary = NSHostingView(rootView: HvfWindowsInstallControls(session: session)
                .buttonStyle(InstallCancellationProbeStyle(probe: ordinaryButtons)))
            ordinary.frame = NSRect(x: 0, y: 0, width: 700, height: 100)
            await fixture.settle(ordinary)
            XCTAssertEqual(ordinaryButtons.entries.count, 2)
            XCTAssertTrue(ordinaryButtons.entries.values.allSatisfy { !$0.enabled })
            XCTAssertFalse(retained.enabled, "Retained controls must not offer cancellation after commit begins")
            retained.activateIfEnabled()
            XCTAssertFalse(session.cancelled)
            XCTAssertEqual(fixture.process.cancellations, 0)
            XCTAssertEqual(fixture.finalizations, 1)
        }
    }

    func testRetainedCancellationDisablesRepeatActionUntilTerminalDismissal() async throws {
        let fixture = try RetainedInstallCancellationFixture(finalizing: false)
        defer { fixture.clean() }
        try await fixture.run { session, host in
            XCTAssertEqual(session.stage, .installing)
            XCTAssertTrue(session.canCancel)
            XCTAssertEqual(fixture.buttons.entries.count, 1)
            let cancel = try XCTUnwrap(fixture.buttons.entries.values.first)
            XCTAssertTrue(cancel.enabled)
            cancel.activateIfEnabled()
            XCTAssertEqual(session.stage, .cancelling)
            XCTAssertFalse(session.canCancel)
            XCTAssertTrue(session.isRunning)
            await fixture.settle(host)
            let pending = try XCTUnwrap(fixture.buttons.entries.values.first)
            XCTAssertFalse(pending.enabled, "An accepted cancellation waits for acknowledgment")
            pending.activateIfEnabled()
            XCTAssertEqual(fixture.process.cancellations, 1)
            let record = try XCTUnwrap(fixture.library.retainedControlRecords.first)
            XCTAssertFalse(fixture.library.dismissRetainedControl(record.id))
            fixture.gate.release()
            for _ in 0..<1000 {
                if !session.isRunning { break }
                try await Task.sleep(nanoseconds: 1_000_000)
            }
            XCTAssertEqual(session.stage, .cancelled)
            XCTAssertFalse(session.isRunning)
            XCTAssertEqual(fixture.finalizations, 0)
            await fixture.settle(host)
            XCTAssertEqual(fixture.buttons.entries.count, 1)
            let dismiss = try XCTUnwrap(fixture.buttons.entries.values.first)
            XCTAssertTrue(dismiss.enabled)
            dismiss.activateIfEnabled()
            XCTAssertTrue(fixture.library.retainedControlRecords.isEmpty)
            XCTAssertNil(fixture.library.selectedID)
        }
    }
}
