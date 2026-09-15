import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallSessionStoreTests: XCTestCase {
    private typealias Fixture = HvfWindowsInstallStoreFixture

    func testRecreatedViewsKeepSessionAcrossSelectionAndReload() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let first = fixture.config("first"), second = fixture.config("second")
        try fixture.save(first)
        try fixture.save(second)
        let library = fixture.library()
        library.selectedID = first.slug
        let original = HvfWindowsInstallView(config: first, library: library, session: library.windowsInstallSession(for: first)).session
        let acknowledgment = try XCTUnwrap(original.start())
        XCTAssertTrue(original.isRunning)
        XCTAssertEqual(original.stage, .validating)
        library.selectedID = second.slug
        let other = HvfWindowsInstallView(config: second, library: library, session: library.windowsInstallSession(for: second)).session
        library.reload()
        library.selectedID = first.slug
        let restored = HvfWindowsInstallView(config: first, library: library, session: library.windowsInstallSession(for: first)).session
        XCTAssertTrue(restored === original)
        XCTAssertFalse(other === original)
        XCTAssertEqual(restored.startedAt, original.startedAt)
        XCTAssertEqual(restored.logLines, original.logLines)
        let duplicate = restored.start()
        XCTAssertNil(duplicate)
        await acknowledgment.value
        await duplicate?.value
        XCTAssertEqual(fixture.jobs.count, 1)
        XCTAssertEqual(fixture.made, 2)
    }

    func testActiveChangedMetadataKeepsOriginalPlanAndInstallRoute() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let originalConfig = fixture.config()
        try fixture.save(originalConfig)
        let library = fixture.library()
        let session = library.windowsInstallSession(for: originalConfig)
        let acknowledgment = try XCTUnwrap(session.start())
        var changed = originalConfig
        changed.bundlePath = fixture.root.appendingPathComponent("replacement-bundle").path
        changed.installPending = false
        changed.backendKind = "fast-vz"
        let loaded: VMConfig
        do {
            try fixture.save(changed, diskGiB: 96)
            library.reload()
            loaded = try XCTUnwrap(library.vms.first)
        } catch {
            await acknowledgment.value
            throw error
        }
        XCTAssertEqual(session.stage, .validating)
        XCTAssertTrue(library.shouldShowWindowsInstall(for: loaded))
        XCTAssertTrue(HvfWindowsInstallView(config: loaded, library: library, session: library.windowsInstallSession(for: loaded)).session === session)
        XCTAssertEqual(session.plan.bundlePath, originalConfig.bundlePath)
        XCTAssertEqual(session.plan.request.diskGiB, 64)
        await acknowledgment.value
        await fixture.finishCancelled(session)
        library.reload()
        XCTAssertFalse(library.shouldShowWindowsInstall(for: loaded))
        changed.installPending = true
        changed.backendKind = "hvf-engine"
        try fixture.save(changed, diskGiB: 96)
        library.reload()
        let replacement = library.windowsInstallSession(for: changed)
        XCTAssertFalse(replacement === session)
        XCTAssertEqual(replacement.plan.bundlePath, changed.bundlePath)
        XCTAssertEqual(replacement.plan.request.diskGiB, 96)
    }

    func testInactiveRequestAndBundleChangesReplaceOnlyOnLookup() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        try fixture.save(config)
        let library = fixture.library()
        let original = library.windowsInstallSession(for: config)
        try fixture.save(config, diskGiB: 96)
        library.reload()
        XCTAssertEqual(fixture.made, 1)
        let changedRequest = library.windowsInstallSession(for: config)
        XCTAssertFalse(changedRequest === original)
        XCTAssertEqual(changedRequest.plan.request.diskGiB, 96)
        config.bundlePath = fixture.root.appendingPathComponent("replacement-bundle").path
        try fixture.save(config, diskGiB: 96)
        library.reload()
        let changedBundle = library.windowsInstallSession(for: config)
        XCTAssertFalse(changedBundle === changedRequest)
        XCTAssertEqual(changedBundle.plan.bundlePath, config.bundlePath)
        XCTAssertEqual(fixture.made, 3)
    }

    func testInactiveRemovedAndCompletedEntriesArePrunedOnReload() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        try fixture.save(config)
        let library = fixture.library()
        let removed = library.windowsInstallSession(for: config)
        try fixture.removeRegistration(config)
        library.reload()
        XCTAssertTrue(library.vms.isEmpty)
        try fixture.save(config)
        library.reload()
        let pending = library.windowsInstallSession(for: config)
        XCTAssertFalse(pending === removed)
        config.installPending = false
        try fixture.save(config)
        library.reload()
        XCTAssertFalse(library.shouldShowWindowsInstall(for: config))
        config.installPending = true
        try fixture.save(config)
        library.reload()
        XCTAssertFalse(library.windowsInstallSession(for: config) === pending)
    }

    func testRemovedActiveEntryIsRetainedUntilTerminalReload() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.config()
        try fixture.save(config)
        let library = fixture.library()
        let session = library.windowsInstallSession(for: config)
        let acknowledgment = try XCTUnwrap(session.start())
        do { try fixture.removeRegistration(config) } catch {
            await acknowledgment.value
            throw error
        }
        library.reload()
        XCTAssertTrue(library.vms.isEmpty)
        // Preservation does not make an externally removed registration visible in the sidebar.
        XCTAssertTrue(library.windowsInstallSession(for: config) === session)
        XCTAssertEqual(session.stage, .validating)
        await acknowledgment.value
        await fixture.finishCancelled(session)
        library.reload()
        try fixture.save(config)
        library.reload()
        XCTAssertFalse(library.windowsInstallSession(for: config) === session)
    }

    func testFailedSessionSurvivesUnchangedReloadAndCanRetry() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.config()
        try fixture.save(config)
        fixture.validator.setError("synthetic validation failure")
        let library = fixture.library()
        let session = library.windowsInstallSession(for: config)
        let acknowledgment = try XCTUnwrap(session.start())
        await acknowledgment.value
        XCTAssertEqual(session.stage, .failed("synthetic validation failure"))
        library.reload()
        let restored = HvfWindowsInstallView(config: config, library: library, session: library.windowsInstallSession(for: config)).session
        XCTAssertTrue(restored === session)
        XCTAssertEqual(restored.logLines, session.logLines)
        fixture.validator.setError(nil)
        let retry = try XCTUnwrap(restored.start())
        await retry.value
        XCTAssertTrue(restored.isRunning)
        XCTAssertEqual(fixture.jobs.count, 1)
    }

    func testSeparateLibrariesDoNotShareSessionsForSameSlug() throws {
        let first = Fixture(), second = Fixture()
        defer { first.clean(); second.clean() }
        try first.save(first.config())
        try second.save(second.config())
        let firstLibrary = first.library(), sameRootLibrary = first.library(), secondLibrary = second.library()
        let session = firstLibrary.windowsInstallSession(for: first.config())
        XCTAssertFalse(sameRootLibrary.windowsInstallSession(for: first.config()) === session)
        let other = secondLibrary.windowsInstallSession(for: second.config())
        XCTAssertFalse(other === session)
        XCTAssertEqual(other.plan.libraryRoot, second.root)
    }

    func testCompletionReloadIsWiredBeforeAppearanceAndDoesNotRetainLibrary() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        try fixture.save(config)
        var library: LibraryModel? = fixture.library()
        weak var weakLibrary = library
        let session = try XCTUnwrap(library).windowsInstallSession(for: config)
        config.installPending = false
        try fixture.save(config)
        XCTAssertEqual(library?.vms.first?.installPending, true)
        XCTAssertNotNil(session.onCompleted)
        session.onCompleted?() // Exercise the real callback without an install or SwiftUI appearance.
        XCTAssertEqual(library?.vms.first?.installPending, false)
        library = nil
        XCTAssertNil(weakLibrary)
    }
}
