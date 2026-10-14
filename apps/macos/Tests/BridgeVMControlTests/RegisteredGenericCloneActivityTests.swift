import XCTest
@testable import BridgeVMControl

@MainActor
final class RegisteredGenericCloneActivityTests: XCTestCase {
    func testRegisteredQemuAliasRefusesUntilObservedActivityEnds() throws {
        let fixture = try RegisteredCloneActivityFixture(alias: true)
        defer { fixture.clean() }
        fixture.model.running = true
        fixture.assertClone(admitted: false)
        fixture.model.running = false
        fixture.assertClone(admitted: true)
    }

    func testAcceptedResourceWorkRefusesCloneUntilCompletion() async throws {
        let fixture = try RegisteredCloneActivityFixture()
        defer { fixture.clean() }
        let gate = DispatchSemaphore(value: 0)
        fixture.backend.resourceGate = gate
        defer { gate.signal() }
        fixture.model.pendingMemGiB = 8; fixture.model.pendingCPU = 4
        fixture.model.applyResources()
        await fixture.storage.wait { fixture.backend.didBeginResourceUpdate }
        XCTAssertTrue(fixture.model.hasAcceptedOperation)
        XCTAssertFalse(fixture.model.running)
        fixture.assertClone(admitted: false)
        gate.signal()
        await fixture.storage.wait { !fixture.model.hasAcceptedOperation }
        XCTAssertEqual(fixture.backend.calls[0], 0)
        fixture.assertClone(admitted: true)
    }

    func testOwnedTPMCleanupRefusesCloneUntilTerminal() async throws {
        let fixture = try RegisteredCloneActivityFixture()
        defer { fixture.clean() }
        let child = try GenericOwnedDelayedChildFixture()
        let plan = GenericOwnedLaunchPlan(primary: child.spec("primary", output: .file(child.root.path)),
            tpm: .init(process: child.spec("tpm"), reusePattern: "owned-fixture", launchDelay: 0))
        let group = GenericOwnedRuntime(plan: plan, spawn: child.spawnAfterTPMReady,
            isDependencyRunning: { _ in false })
        defer { group.requestStop(); child.clean() }
        fixture.model.ownedRuntime = group
        try await child.awaitMarker("tpm", "signal")
        XCTAssertEqual(group.snapshot.phase, .stopping)
        XCTAssertTrue(group.snapshot.isActive)
        XCTAssertFalse(group.snapshot.primaryRunning)
        XCTAssertFalse(fixture.model.running)
        XCTAssertFalse(fixture.model.hasAcceptedOperation)
        fixture.assertClone(admitted: false)
        try child.release("tpm")
        await fixture.storage.wait { !group.snapshot.isActive }
        try child.assertReleased("tpm")
        fixture.assertClone(admitted: true)
    }
}
