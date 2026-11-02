import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfNetworkPreferenceTests: XCTestCase {
    private func drain(_ pending: @MainActor () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while pending(), ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(pending())
    }

    private func ownedArguments(_ config: HvfEngineConfig) -> [String] {
        config.runnerArguments(manifestPath: "m", runnerPath: "r", firmwareCodePath: "f", probePath: "p", ownedRuntime: true)
    }

    /// Baseline on 0b2c7191: "Virtio net" was view state only, and LibraryDashboardPrimaryAction
    /// relaunched from vm.json (networkEnabled ?? true), so an offline guest came back online.
    func testDashboardStartKeepsSavedNetworkOff() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertTrue(session.config.virtioNet, "An absent key keeps the default on")

        // HvfNetworkToggle turned off while stopped saves, rebinds in place and reloads.
        XCTAssertTrue(session.applyNetworkToggle(false) == (false, nil))
        XCTAssertEqual(library.latestConfiguration(for: saved).networkEnabled, false)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session, "Rebound, not replaced")
        XCTAssertFalse(session.config.virtioNet)
        session.guiStartWorker = { _ in HvfGUIStartWorkerOutput(result: .refused("fixture: launch suppressed")) }

        // Exactly LibraryDashboardPrimaryAction.perform(config:model:library:), minus the AppKit window.
        let latest = library.latestConfiguration(for: saved)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        guard case let .accepted(ticket) = session.requestGUIStart(configuration: launch) else {
            return XCTFail("dashboard start refused")
        }
        XCTAssertFalse(ticket.configuration.virtioNet)
        XCTAssertFalse(ticket.configuration.wrapperArguments().contains("--virtio-net"))
        XCTAssertFalse(ownedArguments(ticket.configuration).contains("--virtio-net"))
        try await drain { session.hasPendingGUIStart }
    }

    func testNativeCLIStartIsAdmittedWithSavedNetworkOff() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        XCTAssertNil(try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved)).saveNetworkPreference(false))
        let current = library.latestConfiguration(for: saved)
        XCTAssertEqual(current.networkEnabled, false)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: current))
        XCTAssertEqual(HvfEngineConfig.libraryVM(current, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted), session.config,
            "The CLI's launch must equal the rebound session or it is refused as configurationMismatch")
        session.ownedStartWorker = { _ in .failure(.readiness) }
        let identity = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false).identity
        let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .start,
            requestID: UUID().uuidString, library: identity, vmID: current.slug, appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: current),
            operationID: UUID().uuidString)
        let result = try library.requestOwnedRuntimeStart(request, validateOwner: {}, onReserved: { _ in })
        guard case let .accepted(ticket) = result else { return XCTFail("CLI start refused: \(result)") }
        XCTAssertFalse(ticket.configuration.virtioNet)
        XCTAssertFalse(ownedArguments(ticket.configuration).contains("--virtio-net"))
        try await drain { ticket.workerPending }
    }

    /// App relaunch builds the runtime session from vm.json alone.
    func testRelaunchedLibraryBuildsRuntimeSessionWithSavedNetworkOff() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let first = fixture.library(probe: probe)
        XCTAssertNil(try XCTUnwrap(first.hvfRuntimeDetailSession(for: saved)).saveNetworkPreference(false))
        XCTAssertEqual(first.latestConfiguration(for: saved).networkEnabled, false)
        let relaunched = fixture.library(probe: HvfClipboardSyncProbe())
        let session = try XCTUnwrap(relaunched.hvfRuntimeDetailSession(for: saved))
        XCTAssertFalse(session.config.virtioNet)
        XCTAssertEqual(session.config.libraryContext?.config.networkEnabled, false)
        var pending = relaunched.latestConfiguration(for: saved); pending.installPending = true
        XCTAssertFalse(HvfWindowsBackend(pending, processIsRunning: { _ in false }, libraryRoot: fixture.root)
            .makeHvfEngineConfig().virtioNet, "Backend fallback honours the saved preference")
    }
}
