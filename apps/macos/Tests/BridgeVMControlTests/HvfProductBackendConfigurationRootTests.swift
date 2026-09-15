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

    private func updateResources(_ backend: HvfWindowsBackend, in fixture: Fixture) throws -> VMConfig? {
        let context = try XCTUnwrap(backend.makeHvfEngineConfig().libraryContext)
        XCTAssertEqual(context.rootURL, fixture.rootA)
        // This guard must precede every backend write: the failing baseline cannot write the global library.
        guard context.rootURL == fixture.rootA else { return nil }
        let pathA = fixture.registration(in: fixture.rootA)
        let pathB = fixture.registration(in: fixture.rootB)
        let beforeA = try Data(contentsOf: pathA)
        let beforeB = try Data(contentsOf: pathB)

        XCTAssertTrue(backend.setResources(memMiB: 8192, cpu: 6))

        let afterA = try Data(contentsOf: pathA)
        let saved = try JSONDecoder().decode(VMConfig.self, from: afterA)
        XCTAssertNotEqual(afterA, beforeA)
        XCTAssertEqual(try Data(contentsOf: pathB), beforeB)
        XCTAssertEqual(saved.memMiB, 8192)
        XCTAssertEqual(saved.cpuCount, 6)
        XCTAssertEqual(saved.bundlePath, fixture.config(in: fixture.rootA).bundlePath)
        XCTAssertEqual(backend.config, saved)
        XCTAssertEqual(backend.makeHvfEngineConfig().libraryContext?.config, saved)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.config(in: fixture.rootA).bundlePath))
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.config(in: fixture.rootB).bundlePath))
        return saved
    }

    func testActualConfigFactoryPersistsResourcesOnlyInSuppliedLibrary() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let config = fixture.prepare()
        let backend = try XCTUnwrap(config.makeBackend(libraryRoot: fixture.rootA) as? HvfWindowsBackend)

        _ = try updateResources(backend, in: fixture)
    }

    func testDefaultLibraryFactoryKeepsRootAfterStoppedModelReload() throws {
        let fixture = Fixture()
        defer { fixture.clean() }
        let original = fixture.prepare()
        // Exercise the real default factory; disable its refresh and timer before any model is created.
        let library = LibraryModel(rootURL: fixture.rootA, migrateLegacy: false, startsModelsAutomatically: false)
        library.selectedID = original.slug
        let model = try XCTUnwrap(library.selectedModel)
        let backend = try XCTUnwrap(model.backend as? HvfWindowsBackend)

        guard let saved = try updateResources(backend, in: fixture) else { return }

        XCTAssertEqual(model.config, original)
        XCTAssertFalse(model.running || model.lifecycleBusy || model.busy)
        library.reload()
        let replacement = try XCTUnwrap(library.selectedModel)
        XCTAssertFalse(replacement === model)
        XCTAssertEqual(replacement.config, saved)
        let replacementBackend = try XCTUnwrap(replacement.backend as? HvfWindowsBackend)
        let context = try XCTUnwrap(replacementBackend.makeHvfEngineConfig().libraryContext)
        XCTAssertEqual(context.rootURL, fixture.rootA)
        XCTAssertEqual(context.config, saved)
        XCTAssertEqual(replacementBackend.resources().memMiB, 8192)
        XCTAssertEqual(replacementBackend.resources().cpu, 6)
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
        // The injected model is never started, refreshed, or asked to save resources.
    }
}
