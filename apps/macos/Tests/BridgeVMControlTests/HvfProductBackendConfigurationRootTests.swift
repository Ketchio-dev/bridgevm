import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfProductBackendConfigurationRootTests: XCTestCase {
    private struct Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("backend-root-" + UUID().uuidString)
        var rootA: URL { root.appendingPathComponent("library-a") }
        var rootB: URL { root.appendingPathComponent("library-b") }
        var slug: String { root.lastPathComponent.lowercased() }

        func config(in library: URL) -> VMConfig {
            VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
                bootMode: "windows-hvf", bundlePath: library.appendingPathComponent("bundle").path,
                runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720,
                memMiB: 4096, cpuCount: 2)
        }

        func prepare() -> VMConfig {
            let primary = config(in: rootA)
            var secondary = config(in: rootB)
            secondary.memMiB = 10240
            secondary.cpuCount = 3
            XCTAssertTrue(VMLibrary.save(primary, rootURL: rootA))
            XCTAssertTrue(VMLibrary.save(secondary, rootURL: rootB))
            return primary
        }

        func registration(in library: URL) -> URL {
            library.appendingPathComponent(slug).appendingPathComponent("vm.json")
        }

        func clean() { try? FileManager.default.removeItem(at: root) }
    }

    /// The backend reads the saved resources of the library it was made for, never another library's.
    private func assertReadsSuppliedLibrary(_ backend: HvfWindowsBackend, expected: VMConfig,
                                            in fixture: Fixture) throws {
        let context = try XCTUnwrap(backend.makeHvfEngineConfig().libraryContext)
        XCTAssertEqual(context.rootURL, fixture.rootA)
        XCTAssertEqual(context.config, expected)
        XCTAssertEqual(backend.resources().memMiB, expected.memMiB)
        XCTAssertEqual(backend.resources().cpu, expected.cpuCount)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.config(in: fixture.rootA).bundlePath))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.config(in: fixture.rootB).bundlePath))
    }

    func testActualConfigFactoryReadsResourcesOnlyFromSuppliedLibrary() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.prepare()
        let backend = try XCTUnwrap(config.makeBackend(libraryRoot: fixture.rootA) as? HvfWindowsBackend)

        try assertReadsSuppliedLibrary(backend, expected: config, in: fixture)
    }

    func testDefaultLibraryFactoryKeepsRootAfterStoppedModelReload() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let original = fixture.prepare()
        // Exercise the real default factory; disable its refresh and timer before any model is created.
        let library = LibraryModel(rootURL: fixture.rootA, migrateLegacy: false, startsModelsAutomatically: false)
        library.selectedID = original.slug
        let model = try XCTUnwrap(library.selectedModel)
        try assertReadsSuppliedLibrary(try XCTUnwrap(model.backend as? HvfWindowsBackend), expected: original, in: fixture)

        var saved = original
        saved.memMiB = 8192; saved.cpuCount = 6
        XCTAssertTrue(VMLibrary.save(saved, rootURL: fixture.rootA))
        XCTAssertFalse(model.running)
        library.reload()
        let replacement = try XCTUnwrap(library.selectedModel)
        XCTAssertFalse(replacement === model)
        XCTAssertEqual(replacement.config, saved)
        try assertReadsSuppliedLibrary(try XCTUnwrap(replacement.backend as? HvfWindowsBackend), expected: saved, in: fixture)
    }

    func testExplicitFactoryRemainsAuthoritativeAndCached() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.prepare()
        let backend = HvfWindowsBackend(config, libraryRoot: fixture.rootB)
        let injected = ControlModel(config: config, backend: backend, startsAutomatically: false)
        var received: [VMConfig] = []
        let library = LibraryModel(rootURL: fixture.rootA, migrateLegacy: false,
            startsModelsAutomatically: false, modelFactory: { value in
                received.append(value)
                return injected
            })
        library.selectedID = config.slug

        XCTAssertTrue(try XCTUnwrap(library.selectedModel) === injected)
        XCTAssertTrue(library.model(for: config) === injected)
        XCTAssertEqual(received, [config])
        XCTAssertEqual(backend.makeHvfEngineConfig().libraryContext?.rootURL, fixture.rootB)
        // The injected model is never refreshed.
    }
}
