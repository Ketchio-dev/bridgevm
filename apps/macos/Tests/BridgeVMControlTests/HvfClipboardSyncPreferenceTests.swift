import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfClipboardSyncPreferenceTests: XCTestCase {
    private func drain(_ pending: @MainActor () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while pending(), ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(pending())
    }

    /// Baseline on 8218ee70: the view's sync-off was only an accepted session option, and
    /// LibraryDashboardPrimaryAction relaunched from saved settings with sync on.
    func testDashboardStartDoesNotReenableAcceptedClipboardSyncOff() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))

        // HvfClipboardSyncToggle turned off while stopped saves, rebinds in place and reloads.
        XCTAssertTrue(session.applyClipboardSyncToggle(false) == (false, nil))
        XCTAssertEqual(library.latestConfiguration(for: saved).clipboardSyncEnabled, false)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session, "Rebound, not replaced")
        XCTAssertFalse(session.config.clipboardSync)
        session.guiStartWorker = { _ in HvfGUIStartWorkerOutput(result: .refused("fixture: launch suppressed")) }

        // HvfEngineView Start with the toggle off (same effect as requestGUIStart(currentConfig())).
        var viewStart = session.config; viewStart.clipboardSync = false
        XCTAssertTrue(session.acceptStartConfiguration(viewStart))
        library.reload()
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)

        // Exactly LibraryDashboardPrimaryAction.perform(config:model:library:), minus the AppKit window.
        let latest = library.latestConfiguration(for: saved)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        guard case let .accepted(ticket) = session.requestGUIStart(configuration: launch) else {
            return XCTFail("dashboard start refused")
        }
        XCTAssertFalse(ticket.configuration.clipboardSync)
        XCTAssertFalse(ticket.configuration.wrapperArguments().contains("--agent-clipboard-sync"))
        XCTAssertFalse(session.config.clipboardSync)
        try await drain { session.hasPendingGUIStart }
    }

    /// Baseline on 8218ee70: refused with .configurationMismatch after a view start with sync off.
    func testNativeCLIStartAfterAcceptedClipboardSyncOffIsAdmittedWithSyncOff() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        XCTAssertNil(try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved)).saveClipboardSyncPreference(false))
        let current = library.latestConfiguration(for: saved)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: current))
        XCTAssertEqual(HvfEngineConfig.libraryVM(current, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted), session.config)
        session.ownedStartWorker = { _ in .failure(.readiness) }
        var viewStart = session.config; viewStart.clipboardSync = false
        XCTAssertTrue(session.acceptStartConfiguration(viewStart))
        library.reload()
        let identity = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false).identity
        let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .start,
            requestID: UUID().uuidString, library: identity, vmID: current.slug, appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: current),
            operationID: UUID().uuidString)
        let result = try library.requestOwnedRuntimeStart(request, validateOwner: {}, onReserved: { _ in })
        guard case let .accepted(ticket) = result else { return XCTFail("CLI start refused: \(result)") }
        XCTAssertFalse(ticket.configuration.clipboardSync)
        XCTAssertFalse(ticket.configuration.wrapperArguments().contains("--agent-clipboard-sync"))
        try await drain { ticket.workerPending }
    }

    func testLegacyRegistrationWithoutKeyDecodesNilAndLaunchesWithSyncOn() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let bytes = try Data(contentsOf: fixture.registration(saved))
        XCTAssertFalse(String(decoding: bytes, as: UTF8.self).contains("clipboardSyncEnabled"), "nil is not written")
        let decoded = try JSONDecoder().decode(VMConfig.self, from: bytes)
        XCTAssertNil(decoded.clipboardSyncEnabled)
        XCTAssertEqual(decoded, try NativeLibraryReader.readConfig(rootURL: fixture.root, id: saved.slug))
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(decoded, rootURL: fixture.root))
        XCTAssertTrue(launch.clipboardSync)
        XCTAssertTrue(launch.wrapperArguments().contains("--agent-clipboard-sync"))
        var pending = decoded; pending.installPending = true
        XCTAssertTrue(HvfWindowsBackend(pending, processIsRunning: { _ in false }, libraryRoot: fixture.root)
            .makeHvfEngineConfig().clipboardSync, "Backend fallback keeps the default")
        pending.clipboardSyncEnabled = false
        XCTAssertFalse(HvfWindowsBackend(pending, processIsRunning: { _ in false }, libraryRoot: fixture.root)
            .makeHvfEngineConfig().clipboardSync, "Backend fallback honours the saved preference")
        let library = fixture.library()
        XCTAssertTrue(try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved)).config.clipboardSync)
    }

    func testReenablingWritesAbsentKeyAndRestoresOriginalBytesAndDigest() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let url = fixture.registration(saved), original = try Data(contentsOf: url)
        let library = fixture.library(probe: probe)
        let canonical = library.latestConfiguration(for: saved)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: canonical)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        XCTAssertNil(session.saveClipboardSyncPreference(false))
        XCTAssertTrue(String(decoding: try Data(contentsOf: url), as: UTF8.self).contains("\"clipboardSyncEnabled\" : false"))
        XCTAssertNotEqual(try NativeRuntimeConfigurationIdentity.digest(config: library.latestConfiguration(for: saved)), digest)
        XCTAssertFalse(session.config.clipboardSync)
        XCTAssertTrue(session.applyClipboardSyncToggle(true) == (true, nil))
        XCTAssertEqual(try Data(contentsOf: url), original)
        XCTAssertEqual(library.latestConfiguration(for: saved), canonical)
        XCTAssertEqual(try NativeRuntimeConfigurationIdentity.digest(config: library.latestConfiguration(for: saved)), digest)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
        XCTAssertTrue(session.config.clipboardSync)
        XCTAssertEqual(probe.sessions, 1); XCTAssertEqual(probe.lookups.count, 2)
    }

    func testToggleIsDisabledAndSaveRefusedWhileRunningOrBusy() throws {
        for state in [HvfConnectionState.booting, .connected(host: "fixture"), .stopping, .timedOut] {
            let fixture = try HvfRuntimeSessionStoreFixture()
            defer { fixture.clean() }
            let saved = fixture.config(); fixture.save(saved)
            let url = fixture.registration(saved), original = try Data(contentsOf: url)
            let library = fixture.library()
            let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
            let toggle = HvfClipboardSyncToggle(session: session, isOn: .constant(true))
            XCTAssertFalse(toggle.isDisabled)
            session.connectionState = state
            XCTAssertTrue(toggle.isDisabled, "\(state)")
            XCTAssertNotNil(session.saveClipboardSyncPreference(false), "\(state)")
            XCTAssertTrue(session.applyClipboardSyncToggle(false).isOn, "\(state)")
            XCTAssertEqual(try Data(contentsOf: url), original)
            XCTAssertNil(library.latestConfiguration(for: saved).clipboardSyncEnabled)
            XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session)
            XCTAssertTrue(session.config.clipboardSync)
        }
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library()
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        let reservation = try XCTUnwrap(session.reserveRuntimeMutation(kind: .snapshot))
        XCTAssertTrue(HvfClipboardSyncToggle(session: session, isOn: .constant(true)).isDisabled)
        XCTAssertNotNil(session.saveClipboardSyncPreference(false))
        session.finishRuntimeMutation(reservation)
        session.attachedToExistingProcess = true
        XCTAssertTrue(HvfClipboardSyncToggle(session: session, isOn: .constant(true)).isDisabled)
        XCTAssertNotNil(session.saveClipboardSyncPreference(false), "An attached VM is not stopped")
        session.attachedToExistingProcess = false
        XCTAssertNil(library.latestConfiguration(for: saved).clipboardSyncEnabled)
    }

    func testStaleRegistrationOrUnboundSessionRefusesWithoutWriting() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))
        var external = saved; external.memMiB = 8192; fixture.save(external)
        let url = fixture.registration(saved), changed = try Data(contentsOf: url)
        XCTAssertNotNil(session.saveClipboardSyncPreference(false), "A changed registration is never reverted")
        XCTAssertEqual(try Data(contentsOf: url), changed)
        let unbound = HvfEngineSession(config: session.config, repoRoot: fixture.root, processIsRunning: { _ in false })
        library.reload()
        XCTAssertNotNil(unbound.saveClipboardSyncPreference(false))
        XCTAssertNotNil(session.saveClipboardSyncPreference(false), "A replaced session no longer owns the VM")
        XCTAssertEqual(try Data(contentsOf: url), changed)
        XCTAssertEqual(probe.lookups.count, 1, "Only the first, still-owned attempt reached the process check")
    }
}
