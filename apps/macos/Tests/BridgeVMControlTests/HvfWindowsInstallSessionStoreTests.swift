import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallSessionStoreTests: XCTestCase {
    @MainActor
    private final class Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("install-store-" + UUID().uuidString)
        var jobs: [@MainActor () async -> Void] = []
        var validationError: String?
        var made = 0

        func library() -> LibraryModel {
            LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
                self.made += 1
                return HvfWindowsInstallSession(plan: plan, validate: { _ in self.validationError },
                    schedule: { self.jobs.append($0) })
            })
        }

        func save(_ config: VMConfig, diskGiB: Int = 64) throws {
            let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
                isoSHA256: String(repeating: "a", count: 64), diskGiB: diskGiB, injectViogpu3d: false)
            XCTAssertTrue(request.save(bundlePath: config.bundlePath))
            XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        }

        func config(_ slug: String = "windows") -> VMConfig {
            VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(slug + "/bundle").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720, installPending: true)
        }

        func removeRegistration(_ config: VMConfig) throws {
            try FileManager.default.removeItem(at: root.appendingPathComponent(config.slug + "/vm.json"))
        }

        func finishCancelled(_ session: HvfWindowsInstallSession) async {
            session.cancel()
            let job = jobs.removeFirst()
            await job() // The pre-dispatch cancellation guard returns before the install pipeline.
            XCTAssertFalse(session.isRunning)
        }

        func clean() {
            jobs.removeAll() // Unexecuted work cannot enter the pipeline during teardown.
            try? FileManager.default.removeItem(at: root)
        }
    }

    func testRecreatedViewsKeepSessionAcrossSelectionAndReload() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let first = fixture.config("first"), second = fixture.config("second")
        try fixture.save(first)
        try fixture.save(second)
        let library = fixture.library()
        library.selectedID = first.slug
        let original = HvfWindowsInstallView(config: first, library: library).session
        original.start()
        XCTAssertTrue(original.isRunning)
        library.selectedID = second.slug
        let other = HvfWindowsInstallView(config: second, library: library).session
        library.reload()
        library.selectedID = first.slug
        let restored = HvfWindowsInstallView(config: first, library: library).session
        XCTAssertTrue(restored === original)
        XCTAssertFalse(other === original)
        XCTAssertEqual(restored.startedAt, original.startedAt)
        XCTAssertEqual(restored.logLines, original.logLines)
        restored.start()
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
        session.start()
        var changed = originalConfig
        changed.bundlePath = fixture.root.appendingPathComponent("replacement-bundle").path
        changed.installPending = false
        changed.backendKind = "fast-vz"
        try fixture.save(changed, diskGiB: 96)
        library.reload()
        let loaded = try XCTUnwrap(library.vms.first)
        XCTAssertTrue(library.shouldShowWindowsInstall(for: loaded))
        XCTAssertTrue(HvfWindowsInstallView(config: loaded, library: library).session === session)
        XCTAssertEqual(session.plan.bundlePath, originalConfig.bundlePath)
        XCTAssertEqual(session.plan.request.diskGiB, 64)
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
        session.start()
        try fixture.removeRegistration(config)
        library.reload()
        XCTAssertTrue(library.vms.isEmpty)
        // Preservation does not make an externally removed registration visible in the sidebar.
        XCTAssertTrue(library.windowsInstallSession(for: config) === session)
        await fixture.finishCancelled(session)
        library.reload()
        try fixture.save(config)
        library.reload()
        XCTAssertFalse(library.windowsInstallSession(for: config) === session)
    }

    func testFailedSessionSurvivesUnchangedReloadAndCanRetry() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.config()
        try fixture.save(config)
        fixture.validationError = "synthetic validation failure"
        let library = fixture.library()
        let session = library.windowsInstallSession(for: config)
        session.start()
        XCTAssertEqual(session.stage, .failed("synthetic validation failure"))
        library.reload()
        let restored = HvfWindowsInstallView(config: config, library: library).session
        XCTAssertTrue(restored === session)
        XCTAssertEqual(restored.logLines, session.logLines)
        fixture.validationError = nil
        restored.start()
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
