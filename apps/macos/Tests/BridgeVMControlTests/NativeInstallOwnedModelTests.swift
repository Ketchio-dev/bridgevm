import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOwnedModelTests: XCTestCase {
    private func request(_ operation: NativeInstallControlRequest.Operation, config: VMConfig,
        library: NativeRuntimeLibraryIdentity, app: String, operationID: UUID? = nil
    ) -> NativeInstallControlRequest {
        .init(schema: NativeInstallControlCodec.requestSchema, operation: operation,
            requestID: UUID().uuidString, library: library, vmID: config.slug,
            appInstanceID: app, operationID: operationID?.uuidString)
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
        let app = UUID().uuidString, operationID = UUID()
        let install = request(.install, config: config, library: handle.identity,
            app: app, operationID: operationID)

        let accepted = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertEqual(accepted.disposition, .accepted)
        let existing = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertEqual(existing.disposition, .existing)
        XCTAssertTrue(existing.operation === accepted.operation)
        let observedSession = await waitForSession(fixture, library: library, slug: config.slug)
        let session = try XCTUnwrap(observedSession)
        XCTAssertEqual(try accepted.operation.observation().phase, .preparingSource)
        XCTAssertEqual(fixture.made, 1)

        let status = request(.installStatus, config: config, library: handle.identity, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(status, validateOwner: {}).disposition, .status)
        let cancel = request(.installCancel, config: config, library: handle.identity, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(cancel, validateOwner: {}).disposition, .cancelAccepted)
        XCTAssertEqual(try accepted.operation.observation().phase, .cancelling)
        await fixture.finishCancelled(session)
        XCTAssertEqual(try accepted.operation.observation().phase, .cancelled)
    }

    /// Finalization saves installPending = false before the install reports done, and install-status then
    /// failed with unsupportedTarget, so the CLI could never see `done`.
    func testStatusStillAnswersAfterTheInstallSavedItsFinishedConfiguration() async throws {
        let fixture = HvfWindowsInstallStoreFixture(), config = fixture.config()
        defer { fixture.clean() }
        try fixture.save(config)
        let library = fixture.library()
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false)
        let app = UUID().uuidString
        let accepted = try library.requestOwnedInstall(request(.install, config: config, library: handle.identity,
            app: app, operationID: UUID()), validateOwner: {})
        let observedSession = await waitForSession(fixture, library: library, slug: config.slug)
        let session = try XCTUnwrap(observedSession)
        var finished = config
        finished.installPending = false
        try fixture.save(finished)

        let status = request(.installStatus, config: config, library: handle.identity, app: app)
        let result = try library.requestOwnedInstall(status, validateOwner: {})
        XCTAssertEqual(result.disposition, .status)
        XCTAssertTrue(result.operation === accepted.operation)
        let cancel = request(.installCancel, config: config, library: handle.identity, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(cancel, validateOwner: {}).disposition, .cancelAccepted)
        await fixture.finishCancelled(session)
    }

    /// The CLI publishes vm.json while the app is running; install must find it without a relaunch.
    func testInstallAcceptsAVMPublishedAfterTheAppLaunched() async throws {
        let fixture = HvfWindowsInstallStoreFixture(), config = fixture.config()
        defer { fixture.clean() }
        let library = fixture.library()
        XCTAssertFalse(library.vms.contains { $0.slug == config.slug })
        try fixture.save(config)
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false)
        let app = UUID().uuidString
        let install = request(.install, config: config, library: handle.identity,
            app: app, operationID: UUID())

        let accepted = try library.requestOwnedInstall(install, validateOwner: {})
        XCTAssertEqual(accepted.disposition, .accepted)
        let observedSession = await waitForSession(fixture, library: library, slug: config.slug)
        let session = try XCTUnwrap(observedSession)
        let cancel = request(.installCancel, config: config, library: handle.identity, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(cancel, validateOwner: {}).disposition, .cancelAccepted)
        await fixture.finishCancelled(session)
    }

    /// vm.json changed on disk after the app loaded it (another tool saved it): the app reads the library
    /// once and installs what is saved. Before, the loaded entry no longer matched the file and the CLI's
    /// digest, and the install was refused with `configurationChanged`.
    func testInstallUsesTheCurrentSavedConfigurationAfterVMJsonChangedOnDisk() async throws {
        let fixture = HvfWindowsInstallStoreFixture(), config = fixture.config()
        defer { fixture.clean() }
        try fixture.save(config)
        let library = fixture.library()
        var saved = config
        saved.displayWidth = 1440
        try fixture.save(saved)
        XCTAssertEqual(library.vms.first { $0.slug == config.slug }, config, "the app still holds the old entry")
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false)
        let app = UUID().uuidString

        let accepted = try library.requestOwnedInstall(request(.install, config: config,
            library: handle.identity, app: app, operationID: UUID()), validateOwner: {})
        XCTAssertEqual(accepted.disposition, .accepted)
        let observedSession = await waitForSession(fixture, library: library, slug: config.slug)
        let session = try XCTUnwrap(observedSession)
        XCTAssertEqual(library.windowsInstallSessions.record(for: config.slug)?.sourceConfig, saved)
        let cancel = request(.installCancel, config: config, library: handle.identity, app: app)
        XCTAssertEqual(try library.requestOwnedInstall(cancel, validateOwner: {}).disposition, .cancelAccepted)
        await fixture.finishCancelled(session)
    }

    /// An install started in the window: install-status and install-cancel answered `targetUnavailable`,
    /// and `install` failed as busy, after which install-status reported that failure while Windows
    /// went on installing.
    func testTheCLIFollowsAndCancelsAnInstallStartedInTheWindow() async throws {
        let fixture = HvfWindowsInstallStoreFixture(), config = fixture.config()
        defer { fixture.clean() }
        try fixture.save(config)
        let library = fixture.library()
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false)
        let session = library.windowsInstallSession(for: config)
        let started = try XCTUnwrap(session.start())
        await started.value
        let app = UUID().uuidString

        let status = try library.requestOwnedInstall(request(.installStatus, config: config,
            library: handle.identity, app: app), validateOwner: {})
        XCTAssertEqual(status.disposition, .status)
        XCTAssertEqual(try status.operation.observation().phase, .preparingSource)
        let install = try library.requestOwnedInstall(request(.install, config: config,
            library: handle.identity, app: app, operationID: UUID()), validateOwner: {})
        XCTAssertEqual(install.disposition, .existing)
        XCTAssertTrue(install.operation === status.operation)
        XCTAssertEqual(fixture.made, 1)
        let cancel = try library.requestOwnedInstall(request(.installCancel, config: config,
            library: handle.identity, app: app), validateOwner: {})
        XCTAssertEqual(cancel.disposition, .cancelAccepted)
        XCTAssertEqual(try status.operation.observation().phase, .cancelling)
        await fixture.finishCancelled(session)
        XCTAssertEqual(try status.operation.observation().phase, .cancelled)
    }

    func testConfigurationChangeDuringDetachedPreparationPreventsSessionCreation() async throws {
        let fixture = try HvfWindowsInstallPlanPreparationFixture(gatedCalls: [1])
        let config = fixture.work.config(pending: true)
        fixture.save(config)
        let library = fixture.library()
        let handle = try NativeRuntimeLibraryHandle.open(rootURL: fixture.work.root, create: false)
        let install = request(.install, config: config, library: handle.identity,
            app: UUID().uuidString, operationID: UUID())
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
