import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOwnedModelTests: XCTestCase {
    private func request(_ operation: NativeInstallControlRequest.Operation, config: VMConfig,
        library: NativeRuntimeLibraryIdentity, digest: String, app: String,
        operationID: UUID? = nil
    ) -> NativeInstallControlRequest {
        .init(schema: NativeInstallControlCodec.requestSchema, operation: operation,
            requestID: UUID().uuidString, library: library, vmID: config.slug,
            appInstanceID: app, expectedSavedConfigurationDigest: digest,
            operationID: operationID?.uuidString)
    }

    private func waitForSession(_ fixture: HvfWindowsInstallStoreFixture,
                                library: LibraryModel, slug: String) async -> HvfWindowsInstallSession? {
        for _ in 0..<200 {
            if let session = library.windowsInstallSessions.record(for: slug)?.session,
               session.stage == .preparingSource { return session }
            try? await Task.sleep(nanoseconds: 1_000_000)
        }
        return nil
    }

    func testAcceptedInstallStartsOneRetainedSessionAndCanBeCancelled() async throws {
        let fixture = HvfWindowsInstallStoreFixture(), config = fixture.config()
        defer { fixture.clean() }
        try fixture.save(config)
        let library = fixture.library()
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: config)
        let app = UUID().uuidString, operationID = UUID()
        let install = request(.install, config: config, library: handle.identity,
            digest: digest, app: app, operationID: operationID)

        let accepted = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertEqual(accepted.disposition, .accepted)
        let existing = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertEqual(existing.disposition, .existing)
        XCTAssertTrue(existing.operation === accepted.operation)
        let observedSession = await waitForSession(fixture, library: library, slug: config.slug)
        let session = try XCTUnwrap(observedSession)
        XCTAssertEqual(try accepted.operation.observation().phase, .preparingSource)
        XCTAssertEqual(fixture.made, 1)

        let status = request(.installStatus, config: config, library: handle.identity,
            digest: digest, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(status, validateOwner: {}).disposition, .status)
        let cancel = request(.installCancel, config: config, library: handle.identity,
            digest: digest, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(cancel, validateOwner: {}).disposition, .cancelAccepted)
        XCTAssertEqual(try accepted.operation.observation().phase, .cancelling)
        await fixture.finishCancelled(session)
        XCTAssertEqual(try accepted.operation.observation().phase, .cancelled)
    }

    func testExistingUIInstallAndChangedConfigurationFailClosed() async throws {
        let fixture = HvfWindowsInstallStoreFixture(), config = fixture.config()
        defer { fixture.clean() }
        try fixture.save(config)
        let library = fixture.library()
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: config)
        let session = library.windowsInstallSession(for: config)
        let started = try XCTUnwrap(session.start())
        await started.value
        let install = request(.install, config: config, library: handle.identity,
            digest: digest, app: UUID().uuidString, operationID: UUID())
        XCTAssertThrowsError(try library.requestOwnedInstall(install, validateOwner: {})) {
            XCTAssertEqual($0 as? NativeInstallControlRefusal, .busy)
        }
        session.cancel(); await fixture.finishCancelled(session)

        var changed = config
        changed.displayWidth += 1
        let wrong = request(.install, config: config, library: handle.identity,
            digest: try NativeRuntimeConfigurationIdentity.digest(config: changed),
            app: UUID().uuidString, operationID: UUID())
        XCTAssertThrowsError(try library.requestOwnedInstall(wrong, validateOwner: {})) {
            XCTAssertEqual($0 as? NativeInstallControlRefusal, .configurationChanged)
        }
    }

    func testConfigurationChangeDuringDetachedPreparationPreventsSessionCreation() async throws {
        let fixture = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
        let config = fixture.work.config(pending: true)
        fixture.save(config)
        let library = fixture.library()
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.work.root, create: false)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: config)
        let install = request(.install, config: config, library: handle.identity,
            digest: digest, app: UUID().uuidString, operationID: UUID())
        let accepted = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertEqual(accepted.disposition, .accepted)
        let entered = await fixture.entered(1)
        XCTAssertTrue(entered)

        var changed = config
        changed.displayWidth += 1
        fixture.work.save(changed)
        library.reload()
        fixture.probe.release(1)
        for _ in 0..<200 {
            if try accepted.operation.observation().phase == .failed { break }
            try await Task.sleep(nanoseconds: 1_000_000)
        }
        XCTAssertEqual(try accepted.operation.observation().phase, .failed)
        XCTAssertEqual(fixture.made, 0)
        await fixture.drain()
        fixture.clean()
    }
}
