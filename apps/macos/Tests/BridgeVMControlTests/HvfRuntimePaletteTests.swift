import XCTest
@testable import BridgeVMControl

private final class PaletteEffectBackend: VMBackend {
    var calls = 0
    func isRunning() -> Bool { calls += 1; return false }
    func resources() -> (memMiB: Int, cpu: Int) { calls += 1; return (4096, 2) }
}

@MainActor
final class HvfRuntimePaletteTests: XCTestCase {
    @MainActor
    private final class Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("runtime-palette-" + UUID().uuidString)
        let backend = PaletteEffectBackend()
        let validator = HvfWindowsInstallValidationProbe()
        let queue = HvfWindowsInstallPipelineQueue()
        var modelCreations = 0, runtimeCreations = 0, installCreations = 0, processLookups = 0
        var creations: [Int] { [modelCreations, runtimeCreations, installCreations] }

        func config(kind: String = "hvf-engine", pending: Bool = false) -> VMConfig {
            VMConfig(id: "windows", name: "Windows", displayName: "Windows", backendKind: kind,
                bootMode: "windows-hvf", bundlePath: root.appendingPathComponent("bundle").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: "windows", displayWidth: 1280, displayHeight: 720,
                installPending: pending)
        }

        func save(_ config: VMConfig) {
            XCTAssertTrue(VMLibrary.save(config, rootURL: root))
            let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
                isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
            XCTAssertTrue(request.save(bundlePath: config.bundlePath))
        }

        func library() -> LibraryModel {
            LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
                self.installCreations += 1
                return HvfWindowsInstallSession(plan: plan, validate: { [probe = self.validator] in
                    probe.validate($0)
                }, schedule: self.queue.enqueue, recovery: .init(inspect: { _ in .fresh }))
            }, runtimeSessionFactory: { config in
                self.runtimeCreations += 1
                return HvfEngineSession(config: config, repoRoot: self.root,
                    processIsRunning: { _ in self.processLookups += 1; return false })
            }, modelFactory: { config in
                self.modelCreations += 1
                return ControlModel(config: config, backend: self.backend, startsAutomatically: false)
            })
        }

        func clean() {
            XCTAssertEqual(backend.calls, 0)
            XCTAssertEqual(processLookups, 0)
            queue.discard() // Captured install pipeline closures are never executed.
            try? FileManager.default.removeItem(at: root)
        }
    }

    private func assertNavigation(_ library: LibraryModel, fixture: Fixture, slug: String, name: String) {
        let before = fixture.creations
        var dismissals = 0
        XCTAssertEqual(library.selectedID, slug)
        let commands = library.selectedPaletteCommands(dismiss: { dismissals += 1 })
        let title = "Open controls for \(name)"
        XCTAssertEqual(commands.map(\.title), [title])
        XCTAssertEqual(fixture.creations, before, "Listing a retained control surface must not create another owner")
        guard commands.count == 1, let command = commands.first, command.title == title else { return }
        XCTAssertEqual(command.subtitle, "View startup, shutdown and installation controls")
        XCTAssertEqual(command.systemImage, "desktopcomputer")
        library.selectedID = "a-different-selection"
        library.proMode = true

        command.action()

        XCTAssertEqual(library.selectedID, slug, "Navigation restores the selection captured when the command was built")
        XCTAssertFalse(library.proMode)
        XCTAssertEqual(dismissals, 1)
        XCTAssertEqual(fixture.creations, before)
        XCTAssertEqual(fixture.backend.calls, 0)
        XCTAssertEqual(fixture.processLookups, 0)
    }

    func testCurrentHVFReadyPendingAndStoppedSelectionsOfferOnlyNavigation() throws {
        for pending in [false, true] {
            let fixture = Fixture()
            defer { fixture.clean() }
            let config = fixture.config(pending: pending)
            fixture.save(config)
            let library = fixture.library()
            library.selectedID = config.slug
            assertNavigation(library, fixture: fixture, slug: config.slug, name: config.name)
            XCTAssertEqual(fixture.runtimeCreations, 0)
            XCTAssertEqual(fixture.installCreations, 0)
            if !pending {
                let session = try XCTUnwrap(library.hvfRuntimeSession(for: config))
                let accepted = session.config
                assertNavigation(library, fixture: fixture, slug: config.slug, name: config.name)
                XCTAssertEqual(session.connectionState, .stopped)
                XCTAssertEqual(session.config, accepted)
                XCTAssertTrue(library.hvfRuntimeSession(for: config) === session)
                XCTAssertEqual(fixture.runtimeCreations, 1)
            }
        }
    }

    func testAllActiveRuntimeStatesKeepNavigationAfterMetadataChanges() throws {
        let states: [HvfConnectionState] = [.booting, .connected(host: "fixture"), .stopping, .timedOut]
        for state in states {
            let fixture = Fixture()
            defer { fixture.clean() }
            let original = fixture.config()
            fixture.save(original)
            let library = fixture.library()
            library.selectedID = original.slug
            let session = try XCTUnwrap(library.hvfRuntimeSession(for: original))
            session.connectionState = state
            session.config.ramMiB = 7680
            let accepted = session.config
            var changed = original
            changed.installPending = true
            fixture.save(changed)
            library.reload()
            assertNavigation(library, fixture: fixture, slug: changed.slug, name: changed.name)
            changed.backendKind = "fast-vz"
            changed.name = "Renamed Windows"
            fixture.save(changed)
            library.reload()
            assertNavigation(library, fixture: fixture, slug: changed.slug, name: changed.name)
            XCTAssertTrue(library.hvfRuntimeSession(for: changed) === session)
            XCTAssertEqual(session.connectionState, state)
            XCTAssertEqual(session.config, accepted)
            XCTAssertEqual(fixture.runtimeCreations, 1)
            XCTAssertEqual(fixture.installCreations, 0)
        }
    }

    func testValidatingAndQueuedInstallKeepNavigationAfterMetadataChanges() async throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let original = fixture.config(pending: true)
        fixture.save(original)
        let library = fixture.library()
        library.selectedID = original.slug
        let session = library.windowsInstallSession(for: original)
        let accepted = session.plan
        let acknowledgment = try XCTUnwrap(session.start())
        var changed = original
        changed.backendKind = "qemu-compat"
        changed.installPending = false
        fixture.save(changed)
        library.reload()
        XCTAssertEqual(session.stage, .validating)
        assertNavigation(library, fixture: fixture, slug: changed.slug, name: changed.name)
        XCTAssertTrue(library.windowsInstallSession(for: changed) === session)
        await acknowledgment.value

        XCTAssertEqual(session.stage, .preparingSource)
        XCTAssertEqual(fixture.queue.jobs.count, 1)
        changed.backendKind = "fast-vz"
        fixture.save(changed)
        library.reload()
        assertNavigation(library, fixture: fixture, slug: changed.slug, name: changed.name)
        XCTAssertTrue(library.windowsInstallSession(for: changed) === session)
        XCTAssertEqual(session.plan, accepted)
        XCTAssertEqual(fixture.installCreations, 1)
        XCTAssertEqual(fixture.runtimeCreations, 0)
    }

    func testExperimentalAndEmptySelectionsDoNotConstructGenericModels() {
        let fixture = Fixture()
        defer { fixture.clean() }
        fixture.save(fixture.config(kind: "fast-vz"))
        let library = fixture.library()
        let emptySelections: [String?] = [nil, "missing", LibraryModel.firstRunImportSelectionID]
        for selection in emptySelections {
            library.selectedID = selection
            XCTAssertTrue(library.selectedPaletteCommands(dismiss: {}).isEmpty)
        }
        XCTAssertEqual(fixture.creations, [0, 0, 0])
        library.selectedID = LibraryModel.hvfEngineSelectionID
        assertNavigation(library, fixture: fixture, slug: LibraryModel.hvfEngineSelectionID, name: "HVF Engine")
        XCTAssertEqual(fixture.runtimeCreations, 0)
        let session = library.experimentalHvfRuntimeSession()
        let accepted = session.config
        assertNavigation(library, fixture: fixture, slug: LibraryModel.hvfEngineSelectionID, name: "HVF Engine")
        XCTAssertTrue(library.experimentalHvfRuntimeSession() === session)
        XCTAssertEqual(session.config, accepted)
        XCTAssertEqual(session.connectionState, .stopped)
        XCTAssertEqual(fixture.runtimeCreations, 1)
    }

    func testRetiredEngineSelectionsOfferOnlyNavigation() throws {
        for kind in ["fast-vz", "qemu-compat"] {
            let fixture = Fixture()
            defer { fixture.clean() }
            let config = fixture.config(kind: kind)
            fixture.save(config)
            let library = fixture.library()
            library.selectedID = config.slug
            assertNavigation(library, fixture: fixture, slug: config.slug, name: config.name)
            XCTAssertEqual(fixture.creations, [0, 0, 0])
        }
    }
}
