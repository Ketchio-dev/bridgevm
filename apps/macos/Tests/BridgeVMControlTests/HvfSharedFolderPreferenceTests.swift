import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfSharedFolderPreferenceTests: XCTestCase {
    private func drain(_ pending: @MainActor () -> Bool) async throws {
        let deadline = ProcessInfo.processInfo.systemUptime + 2
        while pending(), ProcessInfo.processInfo.systemUptime < deadline {
            try await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertFalse(pending())
    }

    private func chosenFolder(_ fixture: HvfRuntimeSessionStoreFixture) throws -> HvfSharedFolderSelection {
        let folder = fixture.root.appendingPathComponent("chosen-share", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return HvfSharedFolderSelection(hostPath: folder.path, guestPath: "C:\\bridgevm-share")
    }

    private func value(after flag: String, in arguments: [String]) -> String? {
        guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else { return nil }
        return arguments[index + 1]
    }

    /// Baseline on 0b2c7191: the share was view state only and HvfLibraryLaunchContext built
    /// every dashboard/CLI/relaunch config with shareHostDir: nil, silently dropping it.
    func testChosenFolderIsSavedAndDashboardStartSharesIt() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let library = fixture.library(probe: probe)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved))

        let result = session.applySharedFolderSelection(selection)
        XCTAssertEqual(result.selection, selection); XCTAssertNil(result.message)
        let latest = library.latestConfiguration(for: saved)
        XCTAssertEqual(latest.sharedFolderHostPath, selection.hostPath)
        XCTAssertEqual(latest.sharedFolderGuestPath, selection.guestPath)
        XCTAssertTrue(library.hvfRuntimeDetailSession(for: saved) === session, "Rebound, not replaced")
        XCTAssertEqual(session.config.shareHostDir, selection.hostPath)
        XCTAssertEqual(session.config.shareGuestDir, selection.guestPath)
        session.guiStartWorker = { _ in HvfGUIStartWorkerOutput(result: .refused("fixture: launch suppressed")) }

        // Exactly LibraryDashboardPrimaryAction.perform(config:model:library:), minus the AppKit window.
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(latest, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted))
        guard case let .accepted(ticket) = session.requestGUIStart(configuration: launch) else {
            return XCTFail("dashboard start refused")
        }
        let arguments = ticket.configuration.wrapperArguments()
        XCTAssertEqual(value(after: "--agent-share-host", in: arguments), selection.hostPath)
        XCTAssertEqual(value(after: "--agent-share-guest", in: arguments), selection.guestPath)
        try await drain { session.hasPendingGUIStart }
    }

    func testNativeCLIStartIsAdmittedWithSavedShare() async throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let library = fixture.library(probe: probe)
        XCTAssertNil(try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved)).saveSharedFolderPreference(selection))
        let current = library.latestConfiguration(for: saved)
        XCTAssertEqual(current.sharedFolder, selection)
        let session = try XCTUnwrap(library.hvfRuntimeDetailSession(for: current))
        XCTAssertEqual(HvfEngineConfig.libraryVM(current, rootURL: library.rootURL,
            e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted), session.config)
        session.ownedStartWorker = { _ in .failure(.readiness) }
        let identity = try NativeRuntimeLibraryHandle.open(rootURL: fixture.root, create: false).identity
        let request = NativeRuntimeStartRequest(schema: NativeRuntimeStartCodec.requestSchema, operation: .start,
            requestID: UUID().uuidString, library: identity, vmID: current.slug, appInstanceID: UUID().uuidString,
            expectedSavedConfigurationDigest: try NativeRuntimeConfigurationIdentity.digest(config: current),
            operationID: UUID().uuidString)
        let result = try library.requestOwnedRuntimeStart(request, validateOwner: {}, onReserved: { _ in })
        guard case let .accepted(ticket) = result else { return XCTFail("CLI start refused: \(result)") }
        let arguments = ticket.configuration.runnerArguments(manifestPath: "m", runnerPath: "r",
            firmwareCodePath: "f", probePath: "p", ownedRuntime: true)
        XCTAssertEqual(value(after: "--agent-share-host", in: arguments), selection.hostPath)
        XCTAssertEqual(value(after: "--agent-share-guest", in: arguments), selection.guestPath)
        try await drain { ticket.workerPending }
    }

    /// Sharing is never enabled by default: no key is written and no launch path shares.
    func testRegistrationWithoutSelectionNeverShares() throws {
        let fixture = try HvfRuntimeSessionStoreFixture()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let bytes = try Data(contentsOf: fixture.registration(saved))
        XCTAssertFalse(String(decoding: bytes, as: UTF8.self).contains("sharedFolder"), "nil is not written")
        let decoded = try JSONDecoder().decode(VMConfig.self, from: bytes)
        XCTAssertNil(decoded.sharedFolder)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(decoded, rootURL: fixture.root))
        XCTAssertNil(launch.shareHostDir); XCTAssertNil(launch.shareGuestDir)
        XCTAssertFalse(launch.wrapperArguments().contains("--agent-share-host"))
        let library = fixture.library()
        XCTAssertNil(try XCTUnwrap(library.hvfRuntimeDetailSession(for: saved)).config.shareHostDir)
    }

    /// A relaunched app shares the saved folder, and a saved folder that has since disappeared is
    /// refused by launch readiness rather than dropped or replaced by another folder.
    func testRelaunchKeepsSavedFolderAndMissingFolderFailsReadiness() throws {
        let fixture = try HvfRuntimeSessionStoreFixture(), probe = HvfClipboardSyncProbe()
        defer { fixture.clean() }
        let saved = fixture.config(); fixture.save(saved)
        let selection = try chosenFolder(fixture)
        let first = fixture.library(probe: probe)
        XCTAssertNil(try XCTUnwrap(first.hvfRuntimeDetailSession(for: saved)).saveSharedFolderPreference(selection))
        XCTAssertEqual(first.latestConfiguration(for: saved).sharedFolder, selection)
        let relaunched = fixture.library(probe: HvfClipboardSyncProbe())
        let session = try XCTUnwrap(relaunched.hvfRuntimeDetailSession(for: saved))
        XCTAssertEqual(session.config.shareHostDir, selection.hostPath)
        XCTAssertFalse(session.config.readiness(repoRoot: fixture.root).launchBlockers.map(\.code).contains("share-invalid"))

        try FileManager.default.removeItem(atPath: selection.hostPath)
        let launch = try XCTUnwrap(HvfEngineConfig.libraryVM(relaunched.latestConfiguration(for: saved), rootURL: fixture.root))
        XCTAssertEqual(launch.shareHostDir, selection.hostPath, "Never another folder, never silently dropped")
        XCTAssertTrue(launch.readiness(repoRoot: fixture.root).launchBlockers.map(\.code).contains("share-invalid"))

        var incomplete = relaunched.latestConfiguration(for: saved); incomplete.sharedFolderGuestPath = nil
        XCTAssertNil(incomplete.sharedFolder)
        let partial = try XCTUnwrap(HvfEngineConfig.libraryVM(incomplete, rootURL: fixture.root))
        XCTAssertTrue(partial.readiness(repoRoot: fixture.root).launchBlockers.map(\.code).contains("share-incomplete"))
    }
}
