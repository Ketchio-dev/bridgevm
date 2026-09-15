import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfRuntimeSessionStoreTests: XCTestCase {
    private typealias Fixture = HvfRuntimeSessionStoreFixture

    // Inspect constructed values; only the named preparation host body is explicitly evaluated.
    // Never render, assume private SwiftUI field names, or descend class graphs.
    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let result = value as? T { return [result] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }

    private func runtime(in library: LibraryModel, fixture: Fixture) throws -> HvfEngineView {
        let body = LibraryDetailView(library: library).body
        if !values(HvfWindowsInstallPreparationView.self, in: body).isEmpty {
            fixture.preserveForUnsettledPreparation = true
        }
        let view = try XCTUnwrap(values(HvfEngineView.self, in: body).first)
        XCTAssertTrue(values(ObjectIdentifier.self, in: body).contains(ObjectIdentifier(view.session)))
        XCTAssertTrue(values(HvfWindowsInstallView.self, in: body).isEmpty)
        XCTAssertTrue(values(HvfWindowsInstallPreparationView.self, in: body).isEmpty)
        return view
    }

    func testActualDetailKeepsActiveSessionAcrossNavigationAndReload() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        let first = fixture.config("first"), second = fixture.config("second")
        fixture.save(first); fixture.save(second)
        let library = fixture.library()
        library.selectedID = first.slug
        let original = try runtime(in: library, fixture: fixture).session
        original.connectionState = .connected(host: "fixture")
        original.events = [.unknown("retained event")]
        library.selectedID = second.slug
        XCTAssertFalse(try runtime(in: library, fixture: fixture).session === original)
        library.reload()
        library.selectedID = first.slug
        let restored = try runtime(in: library, fixture: fixture).session
        XCTAssertTrue(restored === original)
        XCTAssertEqual(restored.events, [.unknown("retained event")])
        XCTAssertEqual(fixture.runtimeConfigs.count, 2)
        XCTAssertEqual(fixture.installCount, 0)
    }

    func testAllNonstoppedStatesKeepRuntimeRouteAfterMetadataChanges() throws {
        for state in [HvfConnectionState.booting, .connected(host: "fixture"), .stopping, .timedOut] {
            let fixture = try Fixture()
            defer { fixture.clean() }
            let original = fixture.config()
            fixture.save(original)
            let library = fixture.library()
            let session = try runtime(in: library, fixture: fixture).session
            session.connectionState = state
            var changed = original
            changed.installPending = true
            changed.bundlePath = fixture.root.appendingPathComponent("replacement").path
            for backend in ["hvf-engine", "fast-vz"] {
                changed.backendKind = backend
                fixture.save(changed)
                library.reload()
                XCTAssertTrue(try runtime(in: library, fixture: fixture).session === session, "\(state) / \(backend)")
                XCTAssertEqual(session.connectionState, state)
                XCTAssertEqual(session.config.libraryContext?.config, original)
                XCTAssertEqual(fixture.runtimeConfigs.count, 1)
                XCTAssertEqual(fixture.installCount, 0)
            }
        }
    }

    func testStoppedReplacementChangesActualChildIdentityAndUsesLibraryRoot() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        fixture.save(config)
        let library = fixture.library()
        let original = try runtime(in: library, fixture: fixture).session
        config.bundlePath = fixture.root.appendingPathComponent("replacement").path
        config.cpuCount = 8
        fixture.save(config)
        library.reload()
        XCTAssertEqual(fixture.runtimeConfigs.count, 1, "Replacement is lazy")
        let replacement = try runtime(in: library, fixture: fixture).session
        XCTAssertFalse(replacement === original)
        XCTAssertEqual(replacement.config.smpCpus, 8)
        XCTAssertEqual(replacement.config.uefiVarsPath, config.bundlePath + "/metadata/hvf-vars.fd")
        XCTAssertEqual(replacement.config.libraryContext?.rootURL, fixture.root)
        XCTAssertEqual(fixture.runtimeConfigs.count, 2)
    }

    func testUnchangedRegistrationPreservesAcceptedOptionsAndEvents() throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        fixture.save(fixture.config())
        let library = fixture.library()
        let session = try runtime(in: library, fixture: fixture).session
        var accepted = session.config
        accepted.ramMiB = 8192
        accepted.clipboardSync = false
        XCTAssertTrue(session.acceptStartConfiguration(accepted))
        session.events = [.unknown("stopped event")]
        library.reload()
        XCTAssertTrue(try runtime(in: library, fixture: fixture).session === session)
        XCTAssertEqual(session.config, accepted)
        XCTAssertEqual(session.events, [.unknown("stopped event")])
        XCTAssertEqual(fixture.runtimeConfigs.count, 1)
    }

    func testRemovedStoppedEntryIsPrunedAndRemovedActiveEntryIsRetained() throws {
        for state in [HvfConnectionState.stopped, .timedOut] {
            let fixture = try Fixture()
            defer { fixture.clean() }
            let config = fixture.config()
            fixture.save(config)
            let library = fixture.library()
            let session = try runtime(in: library, fixture: fixture).session
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
            XCTAssertFalse(try runtime(in: library, fixture: fixture).session === session)
        }
    }

    func testStoppedPendingOrOtherBackendEntriesArePruned() async throws {
        for becomesPending in [true, false] {
            let fixture = try Fixture()
            defer { fixture.clean() }
            var config = fixture.config()
            fixture.save(config)
            let library = fixture.library()
            let session = try runtime(in: library, fixture: fixture).session
            config.installPending = becomesPending
            config.backendKind = becomesPending ? "hvf-engine" : "fast-vz"
            fixture.save(config)
            library.reload()
            let body = LibraryDetailView(library: library).body
            XCTAssertTrue(values(HvfEngineView.self, in: body).isEmpty)
            let hosts = values(HvfWindowsInstallPreparationView.self, in: body)
            if becomesPending && hosts.count != 1 { fixture.preserveForUnsettledPreparation = true }
            for host in hosts { await host.preparation.preparationTask?.value }
            XCTAssertEqual(hosts.count, becomesPending ? 1 : 0)
            if let host = hosts.first {
                XCTAssertEqual(values(HvfWindowsInstallView.self, in: host.body).count, 1)
            }
            config.installPending = false
            config.backendKind = "hvf-engine"
            fixture.save(config)
            library.reload()
            XCTAssertFalse(try runtime(in: library, fixture: fixture).session === session)
        }
    }

    func testExperimentalVMAndLibraryInstancesOwnIndependentSessions() throws {
        let first = try Fixture(), second = try Fixture()
        defer { first.clean(); second.clean() }
        first.save(first.config("experimental")); second.save(second.config("experimental"))
        let library = first.library(), sameRoot = first.library(), otherRoot = second.library()
        let vm = try runtime(in: library, fixture: first).session
        XCTAssertFalse(try runtime(in: sameRoot, fixture: first).session === vm)
        let other = try runtime(in: otherRoot, fixture: second).session
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
        let fixture = try Fixture()
        defer { fixture.clean() }
        fixture.save(fixture.config())
        var library: LibraryModel? = fixture.library()
        var view: HvfEngineView? = try runtime(in: XCTUnwrap(library), fixture: fixture)
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
        let fixture = try Fixture()
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
        XCTAssertEqual(try runtime(in: library, fixture: fixture).session.config.libraryContext?.config, changed)
        XCTAssertEqual(library.selectedDetail?.config, changed)
        XCTAssertTrue(library.selectedDetail?.model === model)
    }

    func testActiveInstallStillOwnsActualDetailAfterReadyMetadataAppears() async throws {
        let fixture = try Fixture()
        defer { fixture.clean() }
        var config = fixture.config()
        config.installPending = true
        fixture.save(config)
        let library = fixture.library()
        let body = LibraryDetailView(library: library).body
        let hosts = values(HvfWindowsInstallPreparationView.self, in: body)
        if hosts.count != 1 { fixture.preserveForUnsettledPreparation = true }
        for host in hosts { await host.preparation.preparationTask?.value }
        let host = try XCTUnwrap(hosts.first)
        let install = try XCTUnwrap(values(HvfWindowsInstallView.self, in: host.body).first).session
        await install.start()?.value // Injected scheduler retains the work without executing it.
        config.installPending = false
        fixture.save(config)
        library.reload()
        let recreated = LibraryDetailView(library: library).body
        let restoredHosts = values(HvfWindowsInstallPreparationView.self, in: recreated)
        if restoredHosts.count != 1 { fixture.preserveForUnsettledPreparation = true }
        let restored = try XCTUnwrap(restoredHosts.first)
        guard case let .ready(current) = restored.preparation.state else {
            XCTFail("An active install must be immediately ready without a second preparation await")
            for host in restoredHosts { await host.preparation.preparationTask?.value }
            return // Drain only after recording the failure, never to turn it into a pass.
        }
        XCTAssertTrue(current === install)
        XCTAssertTrue(try XCTUnwrap(values(HvfWindowsInstallView.self, in: restored.body).first).session === install)
        XCTAssertTrue(values(HvfEngineView.self, in: recreated).isEmpty)
        XCTAssertEqual(fixture.runtimeConfigs.count, 0)
        XCTAssertEqual(fixture.installCount, 1)
        XCTAssertEqual(fixture.installJobs.count, 1)
    }
}
