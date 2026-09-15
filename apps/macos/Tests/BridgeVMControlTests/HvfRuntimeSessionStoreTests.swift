import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeSessionStoreTests: XCTestCase {
    @MainActor
    private final class Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("runtime-store-" + UUID().uuidString)
        var runtimeConfigs: [HvfEngineConfig] = []
        var installCount = 0
        var processLookups = 0
        var installJobs: [@MainActor () async -> Void] = []

        func library() -> LibraryModel {
            LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
                self.installCount += 1
                return HvfWindowsInstallSession(plan: plan, validate: { _ in nil },
                    schedule: { self.installJobs.append($0) })
            }, runtimeSessionFactory: { config in
                self.runtimeConfigs.append(config)
                return HvfEngineSession(config: config, repoRoot: self.root,
                    processIsRunning: { _ in self.processLookups += 1; return false })
            }, modelFactory: { ControlModel(config: $0, startsAutomatically: false) })
        }

        func config(_ slug: String = "windows") -> VMConfig {
            VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(slug + "/bundle").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720, installPending: false)
        }

        func save(_ config: VMConfig) {
            XCTAssertTrue(VMLibrary.save(config, rootURL: root))
            let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
                isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
            XCTAssertTrue(request.save(bundlePath: config.bundlePath))
        }

        func removeRegistration(_ config: VMConfig) throws {
            try FileManager.default.removeItem(at: root.appendingPathComponent(config.slug + "/vm.json"))
        }

        func clean() {
            XCTAssertEqual(processLookups, 0, "Constructing detail values must not attach or launch")
            installJobs.removeAll() // No queued install work is ever executed.
            try? FileManager.default.removeItem(at: root)
        }
    }

    // Inspect only the constructed view values; never render or evaluate child bodies.
    // No private SwiftUI type or field names are assumed, and class graphs are excluded.
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func runtime(in library: LibraryModel) throws -> HvfEngineView {
        let body = LibraryDetailView(library: library).body
        let view = try XCTUnwrap(values(HvfEngineView.self, in: body).first)
        XCTAssertTrue(values(ObjectIdentifier.self, in: body).contains(ObjectIdentifier(view.session)))
        XCTAssertTrue(values(HvfWindowsInstallView.self, in: body).isEmpty)
        return view
    }

    func testActualDetailKeepsActiveSessionAcrossNavigationAndReload() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let first = fixture.config("first"), second = fixture.config("second")
        fixture.save(first); fixture.save(second)
        let library = fixture.library()
        library.selectedID = first.slug
        let original = try runtime(in: library).session
        original.connectionState = .connected(host: "fixture")
        original.events = [.unknown("retained event")]
        library.selectedID = second.slug
        XCTAssertFalse(try runtime(in: library).session === original)
        library.reload()
        library.selectedID = first.slug
        let restored = try runtime(in: library).session
        XCTAssertTrue(restored === original)
        XCTAssertEqual(restored.events, [.unknown("retained event")])
        XCTAssertEqual(fixture.runtimeConfigs.count, 2)
        XCTAssertEqual(fixture.installCount, 0)
    }

    func testAllNonstoppedStatesKeepRuntimeRouteAfterMetadataChanges() throws {
        for state in [HvfConnectionState.booting, .connected(host: "fixture"), .stopping, .timedOut] {
            let fixture = Fixture()
            defer { fixture.clean() }
            let original = fixture.config()
            fixture.save(original)
            let library = fixture.library()
            let session = try runtime(in: library).session
            session.connectionState = state
            var changed = original
            changed.installPending = true
            changed.bundlePath = fixture.root.appendingPathComponent("replacement").path
            for backend in ["hvf-engine", "fast-vz"] {
                changed.backendKind = backend
                fixture.save(changed)
                library.reload()
                XCTAssertTrue(try runtime(in: library).session === session, "\(state) / \(backend)")
                XCTAssertEqual(session.connectionState, state)
                XCTAssertEqual(session.config.libraryContext?.config, original)
                XCTAssertEqual(fixture.runtimeConfigs.count, 1)
                XCTAssertEqual(fixture.installCount, 0)
            }
        }
    }

    func testStoppedReplacementChangesActualChildIdentityAndUsesLibraryRoot() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        fixture.save(config)
        let library = fixture.library()
        let original = try runtime(in: library).session
        config.bundlePath = fixture.root.appendingPathComponent("replacement").path
        config.cpuCount = 8
        fixture.save(config)
        library.reload()
        XCTAssertEqual(fixture.runtimeConfigs.count, 1, "Replacement is lazy")
        let replacement = try runtime(in: library).session
        XCTAssertFalse(replacement === original)
        XCTAssertEqual(replacement.config.smpCpus, 8)
        XCTAssertEqual(replacement.config.uefiVarsPath, config.bundlePath + "/metadata/hvf-vars.fd")
        XCTAssertEqual(replacement.config.libraryContext?.rootURL, fixture.root)
        XCTAssertEqual(fixture.runtimeConfigs.count, 2)
    }

    func testUnchangedRegistrationPreservesAcceptedOptionsAndEvents() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        fixture.save(fixture.config())
        let library = fixture.library()
        let session = try runtime(in: library).session
        var accepted = session.config
        accepted.ramMiB = 8192
        accepted.clipboardSync = false
        XCTAssertTrue(session.acceptStartConfiguration(accepted))
        session.events = [.unknown("stopped event")]
        library.reload()
        XCTAssertTrue(try runtime(in: library).session === session)
        XCTAssertEqual(session.config, accepted)
        XCTAssertEqual(session.events, [.unknown("stopped event")])
        XCTAssertEqual(fixture.runtimeConfigs.count, 1)
    }

    func testRemovedStoppedEntryIsPrunedAndRemovedActiveEntryIsRetained() throws {
        for state in [HvfConnectionState.stopped, .timedOut] {
            let fixture = Fixture()
            defer { fixture.clean() }
            let config = fixture.config()
            fixture.save(config)
            let library = fixture.library()
            let session = try runtime(in: library).session
            session.connectionState = state
            try fixture.removeRegistration(config)
            library.reload()
            XCTAssertTrue(library.vms.isEmpty) // Retention does not restore a removed sidebar row.
            if state != .stopped {
                XCTAssertTrue(library.hvfRuntimeSession(for: config) === session)
                session.connectionState = .stopped
                library.reload()
            }
            fixture.save(config)
            library.reload()
            library.selectedID = config.slug
            XCTAssertFalse(try runtime(in: library).session === session)
        }
    }

    func testStoppedPendingOrOtherBackendEntriesArePruned() throws {
        for becomesPending in [true, false] {
            let fixture = Fixture()
            defer { fixture.clean() }
            var config = fixture.config()
            fixture.save(config)
            let library = fixture.library()
            let session = try runtime(in: library).session
            config.installPending = becomesPending
            config.backendKind = becomesPending ? "hvf-engine" : "fast-vz"
            fixture.save(config)
            library.reload()
            let body = LibraryDetailView(library: library).body
            XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
            XCTAssertEqual(values(HvfWindowsInstallView.self, in: body).count, becomesPending ? 1 : 0)
            config.installPending = false
            config.backendKind = "hvf-engine"
            fixture.save(config)
            library.reload()
            XCTAssertFalse(try runtime(in: library).session === session)
        }
    }

    func testExperimentalVMAndLibraryInstancesOwnIndependentSessions() throws {
        let first = Fixture(), second = Fixture()
        defer { first.clean(); second.clean() }
        first.save(first.config("experimental")); second.save(second.config("experimental"))
        let library = first.library(), sameRoot = first.library(), otherRoot = second.library()
        let vm = try runtime(in: library).session
        XCTAssertFalse(try runtime(in: sameRoot).session === vm)
        let other = try runtime(in: otherRoot).session
        XCTAssertFalse(other === vm)
        XCTAssertEqual(other.config.libraryContext?.rootURL, second.root)
        library.selectedID = LibraryModel.hvfEngineSelectionID
        let body = LibraryDetailView(library: library).body
        let experimental = try XCTUnwrap(values(HvfEngineView.self, in: body).first).session
        XCTAssertFalse(experimental === vm)
        experimental.config.ramMiB = 8192
        library.reload()
        let recreated = LibraryDetailView(library: library).body
        XCTAssertTrue(try XCTUnwrap(values(HvfEngineView.self, in: recreated).first).session === experimental)
        XCTAssertEqual(experimental.config.ramMiB, 8192)
    }

    func testLibraryRetainsSessionAfterDetailAndDisplaySurfaceValuesAreReleased() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        fixture.save(fixture.config())
        var library: LibraryModel? = fixture.library()
        var view: HvfEngineView? = try runtime(in: XCTUnwrap(library))
        weak var observed = view?.session
        observed?.connectionState = .booting
        view = nil
        XCTAssertNotNil(observed)
        var surface: HvfLiveDisplaySurface? = HvfLiveDisplaySurface(session: try XCTUnwrap(observed))
        XCTAssertTrue(surface?.session === observed)
        surface = nil
        XCTAssertNotNil(observed)
        library = nil
        XCTAssertNil(observed)
    }

    func testBusyGenericModelDefersMetadataThenNextDetailUsesLatestWithoutReload() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        var original = fixture.config()
        original.backendKind = "fast-vz"
        fixture.save(original)
        let library = fixture.library()
        let model = try XCTUnwrap(library.selectedModel)
        model.busy = true
        var changed = original
        changed.backendKind = "hvf-engine"
        changed.bundlePath = fixture.root.appendingPathComponent("replacement").path
        fixture.save(changed)
        library.reload()
        let busyBody = LibraryDetailView(library: library).body
        XCTAssertTrue(values(HvfEngineView.self, in: busyBody).isEmpty)
        XCTAssertTrue(try XCTUnwrap(values(VMDetailPanel.self, in: busyBody).first).model === model)
        XCTAssertEqual(library.selectedDetail?.config, original)
        XCTAssertEqual(fixture.runtimeConfigs.count, 0)
        model.busy = false
        XCTAssertEqual(library.selectedModel?.config, original, "The control cache is still stale")
        XCTAssertEqual(try runtime(in: library).session.config.libraryContext?.config, changed)
        XCTAssertEqual(library.selectedDetail?.config, changed)
        XCTAssertTrue(library.selectedDetail?.model === model)
    }

    func testActiveInstallStillOwnsActualDetailAfterReadyMetadataAppears() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        config.installPending = true
        fixture.save(config)
        let library = fixture.library()
        let body = LibraryDetailView(library: library).body
        let install = try XCTUnwrap(values(HvfWindowsInstallView.self, in: body).first).session
        await install.start()?.value // Injected scheduler retains the work without executing it.
        config.installPending = false
        fixture.save(config)
        library.reload()
        let recreated = LibraryDetailView(library: library).body
        XCTAssertTrue(try XCTUnwrap(values(HvfWindowsInstallView.self, in: recreated).first).session === install)
        XCTAssertTrue(values(HvfEngineView.self, in: recreated).isEmpty)
        XCTAssertEqual(fixture.runtimeConfigs.count, 0)
        XCTAssertEqual(fixture.installCount, 1)
        XCTAssertEqual(fixture.installJobs.count, 1)
    }
}
