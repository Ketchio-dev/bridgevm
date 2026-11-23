import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeAppLifecycleJourneyTests: XCTestCase {
    func testPendingFailureRetryAndInstalledStartBoundaryShareExactIdentity() throws {
        let fixture = try HvfRuntimeWorkAdmissionFixture()
        defer { fixture.clean() }
        let pending = fixture.config(pending: true)
        fixture.save(pending); fixture.saveRequest(pending)
        let model = fixture.library()
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: pending)
        let store = model.windowsInstallSessions.cliOperations
        let firstID = UUID(), retryID = UUID()

        guard case let .accepted(first) = store.reserve(vmID: pending.slug, digest: digest,
            operationID: firstID) else { return XCTFail("initial install was not admitted") }
        first.fail("synthetic host validation failure")
        guard case let .existing(replayed) = store.reserve(vmID: pending.slug, digest: digest,
            operationID: firstID) else { return XCTFail("terminal operation was not replayed") }
        guard case let .accepted(retry) = store.reserve(vmID: pending.slug, digest: digest,
            operationID: retryID) else { return XCTFail("explicit retry was not admitted") }
        XCTAssertTrue(first === replayed); XCTAssertFalse(first === retry)
        XCTAssertEqual(try retry.observation().phase, .preparingPlan)

        let pendingReadiness = try NativeCLIReadiness.snapshot(rootURL: fixture.root,
            id: pending.slug, repoRoot: fixture.runtimeRepo)
        XCTAssertFalse(pendingReadiness.engineChecksPerformed)
        XCTAssertEqual(pendingReadiness.launchBlockers.map(\.code), ["installation-pending"])
        guard case .refused = model.requestCLIStart(vmID: pending.slug) else { return XCTFail("pending install started") }

        XCTAssertTrue(retry.cancel())
        var installed = pending; installed.installPending = false
        fixture.save(installed); model.reload()
        XCTAssertFalse(try NativeCLIReadiness.snapshot(rootURL: fixture.root,
            id: installed.slug, repoRoot: fixture.runtimeRepo).launchBlockers
            .contains { $0.code == "installation-pending" })
        guard case .failure(.targetUnavailable) = store.operation(vmID: pending.slug,
            digest: digest) else { return XCTFail("completed registration retained stale install lookup") }

        guard case let .accepted(ticket, _) = model.requestCLIStart(vmID: installed.slug) else {
            return XCTFail("installed registration was not admitted")
        }
        ticket.invalidate("test ends before any start effect")
        XCTAssertEqual(fixture.runtimeCreations, 1)
    }
}
