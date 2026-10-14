import XCTest
@testable import BridgeVMControl

final class GenericOwnedRuntimeFailureTests: XCTestCase {
    func testStopWaitsForPrimaryExitBeforeSignallingOwnedTPM() async throws {
        let fixture = try GenericOwnedDelayedChildFixture()
        let plan = GenericOwnedLaunchPlan(primary: fixture.spec("primary"),
            tpm: .init(process: fixture.spec("tpm"), reusePattern: "owned-fixture", launchDelay: 0))
        let group = GenericOwnedRuntime(plan: plan, spawn: fixture.spawnAfterTPMReady,
            isDependencyRunning: { _ in false })
        defer { group.requestStop(); fixture.clean() }
        try await fixture.awaitMarker("primary", "ready")
        group.requestStop()
        try await fixture.awaitMarker("primary", "signal")
        let children = fixture.recorder.processes
        guard children.count == 2 else { return XCTFail("The group must own primary and TPM") }
        XCTAssertEqual(group.snapshot.phase, .stopping)
        XCTAssertTrue(group.snapshot.isActive)
        XCTAssertTrue(group.snapshot.primaryRunning)
        XCTAssertFalse(fixture.has("tpm", "signal"))
        try? await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertFalse(fixture.has("tpm", "signal"), "An owned TPM stays alive throughout primary shutdown")
        XCTAssertTrue(children.allSatisfy(\.isRunning))

        try fixture.release("primary")
        try await fixture.awaitMarker("tpm", "signal")
        XCTAssertFalse(group.snapshot.primaryRunning)
        XCTAssertEqual(group.snapshot.phase, .stopping)
        XCTAssertTrue(group.snapshot.isActive, "Outstanding owned TPM cleanup reserves the runtime")
        XCTAssertFalse(children[1].isRunning)
        XCTAssertTrue(children[0].isRunning)
        try fixture.assertReleased("primary")
        try fixture.release("tpm")
        await awaitTerminal(group)
        try fixture.assertReleased("tpm")
        XCTAssertNil(group.snapshot.failure)
        XCTAssertTrue(fixture.recorder.processes.allSatisfy { !$0.isRunning })
    }

    func testPrimaryLaunchRefusalKeepsOwnedTPMCleanupActiveUntilExit() async throws {
        let fixture = try GenericOwnedDelayedChildFixture()
        // An actual launcher output refusal occurs after the owned TPM is ready.
        let plan = GenericOwnedLaunchPlan(primary: fixture.spec("primary", output: .file(fixture.root.path)),
            tpm: .init(process: fixture.spec("tpm"), reusePattern: "owned-fixture", launchDelay: 0))
        let group = GenericOwnedRuntime(plan: plan, spawn: fixture.spawnAfterTPMReady,
            isDependencyRunning: { _ in false })
        defer { group.requestStop(); fixture.clean() }
        try await fixture.awaitMarker("tpm", "signal")
        XCTAssertEqual(fixture.recorder.processes.count, 1)
        let dependency = try XCTUnwrap(fixture.recorder.processes.first)
        XCTAssertTrue(dependency.isRunning)
        XCTAssertFalse(fixture.has("primary", "ready"))
        XCTAssertNotNil(group.snapshot.failure)
        XCTAssertEqual(group.snapshot.phase, .stopping)
        XCTAssertTrue(group.snapshot.isActive)
        XCTAssertFalse(group.snapshot.primaryRunning)
        try? await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertTrue(group.snapshot.isActive, "Launch failure cannot report terminal before auxiliary exit")
        try fixture.release("tpm")
        await awaitTerminal(group)
        try fixture.assertReleased("tpm")
        XCTAssertNotNil(group.snapshot.failure)
        XCTAssertFalse(dependency.isRunning)
    }

    private func awaitTerminal(_ group: GenericOwnedRuntime) async {
        let deadline = ProcessInfo.processInfo.systemUptime + 1.5
        while group.snapshot.isActive, ProcessInfo.processInfo.systemUptime < deadline {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertEqual(group.snapshot.phase, .terminal)
        XCTAssertFalse(group.snapshot.isActive)
    }
}
