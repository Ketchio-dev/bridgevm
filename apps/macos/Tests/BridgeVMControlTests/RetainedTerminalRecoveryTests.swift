import XCTest
@testable import BridgeVMControl

@MainActor
final class RetainedTerminalRecoveryTests: XCTestCase {
    func testFastVZDismissalReopensCurrentRegistrationWithFreshControl() async throws {
        try await assertTerminalRecovery(kind: "fast-vz")
    }

    func testQemuDismissalReopensCurrentRegistrationWithFreshControl() async throws {
        try await assertTerminalRecovery(kind: "qemu-compat")
    }

    private func assertTerminalRecovery(kind: String) async throws {
        let f = try RetainedTerminalRecoveryFixture(kind: kind)
        let (current, record) = try f.replaceWhileObservedRunning()
        let bytes = try Data(contentsOf: f.registration)
        let retained = try f.retainedView(record)
        f.accepted.applyRuntimeStatus(running: false, ip: nil)
        XCTAssertFalse(record.descriptor.isActive)
        retained.dismiss()
        XCTAssertTrue(f.library.retainedControlRecords.isEmpty)
        XCTAssertEqual(f.library.selectedID, current.slug)

        let controls = try f.ordinaryControls()
        XCTAssertFalse(controls.model === f.accepted, "Current registration needs its own ordinary control")
        XCTAssertEqual(controls.model.config, current)
        XCTAssertFalse(controls.model.isRetained)
        try await f.start(controls)
        XCTAssertEqual(f.factory.backends.last?.startCalls, 1, "Actual ordinary Start must reach only the current fake backend")
        XCTAssertEqual(f.factory.backends.first?.startCalls, 0)
        f.accepted.start()
        XCTAssertEqual(f.factory.backends.first?.startCalls, 0, "The old retained handle must remain unable to relaunch")
        XCTAssertTrue(f.accepted.isRetained)
        XCTAssertEqual(try Data(contentsOf: f.registration), bytes)
        XCTAssertTrue(f.factory.backends.allSatisfy { $0.forbiddenCalls == 0 })
    }

    func testActiveRetainedHandleCannotBeDismissedOrReplaced() throws {
        let f = try RetainedTerminalRecoveryFixture()
        let (current, record) = try f.replaceWhileObservedRunning()
        let bytes = try Data(contentsOf: f.registration)
        let retained = try f.retainedView(record)
        retained.dismiss()
        XCTAssertEqual(f.library.selectedID, record.id)
        XCTAssertEqual(f.library.retainedControlRecords.map(\.id), [record.id])
        XCTAssertTrue(f.library.model(for: current) === f.accepted)
        XCTAssertEqual(f.factory.models.count, 1)
        f.accepted.start()
        XCTAssertEqual(f.factory.backends[0].startCalls, 0)
        XCTAssertEqual(try Data(contentsOf: f.registration), bytes)
    }

    func testDismissingOldRecordPreservesAlreadyCurrentCachedControl() throws {
        let f = try RetainedTerminalRecoveryFixture()
        let (current, record) = try f.replaceWhileObservedRunning()
        f.accepted.applyRuntimeStatus(running: false, ip: nil)
        f.library.reload() // Existing explicit reload already retires the terminal cache entry.
        let ordinary = f.library.model(for: current)
        XCTAssertFalse(ordinary === f.accepted)
        let bytes = try Data(contentsOf: f.registration)
        try f.retainedView(record).dismiss()
        XCTAssertTrue(f.library.retainedControlRecords.isEmpty)
        XCTAssertTrue(f.library.model(for: current) === ordinary)
        XCTAssertFalse(ordinary.isRetained)
        XCTAssertEqual(f.factory.models.count, 2)
        XCTAssertEqual(try Data(contentsOf: f.registration), bytes)
    }

    func testOrdinaryIdleRegistrationKeepsExistingControl() throws {
        let f = try RetainedTerminalRecoveryFixture()
        XCTAssertFalse(f.accepted.hasActiveRuntimeWork)
        XCTAssertFalse(f.accepted.isRetained)
        let controls = try f.ordinaryControls()
        XCTAssertTrue(controls.model === f.accepted)
        XCTAssertTrue(f.library.model(for: f.original) === f.accepted)
        XCTAssertEqual(f.factory.models.count, 1)
        XCTAssertTrue(f.library.retainedControlRecords.isEmpty)
    }
}
