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
        assertStartRefused(.unsupportedRuntime, model: model,
            request: try startRequest(config: pending, root: fixture.root))

        XCTAssertTrue(retry.cancel())
        var installed = pending; installed.installPending = false
        fixture.save(installed); model.reload()
        XCTAssertFalse(try NativeCLIReadiness.snapshot(rootURL: fixture.root,
            id: installed.slug, repoRoot: fixture.runtimeRepo).launchBlockers
            .contains { $0.code == "installation-pending" })
        guard case .failure(.targetUnavailable) = store.operation(vmID: pending.slug,
            digest: digest) else { return XCTFail("completed registration retained stale install lookup") }

        let result = try model.requestOwnedRuntimeStart(
            try startRequest(config: installed, root: fixture.root), validateOwner: {},
            onReserved: { _ in throw NativeRuntimeError.timedOut })
        // The shared safety fixture rewrites the synthetic swtpm path, so the
        // retained session refuses that launch at the later exact-config gate.
        guard case .refused(.configurationMismatch) = result else {
            return XCTFail("installed registration result: \(result)")
        }
        XCTAssertEqual(fixture.runtimeCreations, 1)
    }

    private func startRequest(config: VMConfig, root: URL) throws -> NativeRuntimeStartRequest {
        let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false).identity
        return NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema,
            operation: .start, requestID: UUID().uuidString, library: library,
            vmID: config.slug, appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: config),
            operationID: UUID().uuidString)
    }

    private func assertStartRefused(_ expected: NativeRuntimeStartRefusal,
                                    model: LibraryModel, request: NativeRuntimeStartRequest) {
        XCTAssertThrowsError(try model.requestOwnedRuntimeStart(request,
            validateOwner: {}, onReserved: { _ in XCTFail("refused start reserved work") })) {
            XCTAssertEqual($0 as? NativeRuntimeStartRefusal, expected)
        }
    }
}
