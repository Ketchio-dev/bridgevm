import XCTest
@testable import BridgeVMControl

@MainActor
final class FirstRunImportNavigationTests: XCTestCase {
    private let importSelection = LibraryModel.firstRunImportSelectionID

    func testReloadPreservesImportSelectionWithAnotherVMAndProMode() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = VMConfig(id: "existing", name: "Existing", displayName: "Existing",
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: root.appendingPathComponent("existing/bundle.vmbridge").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "existing", displayWidth: 1280, displayHeight: 800)
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let library = LibraryModel(rootURL: root, migrateLegacy: false,
            modelFactory: { ControlModel(config: $0, startsAutomatically: false) })
        library.selectedID = importSelection
        library.proMode = true

        library.reload()

        XCTAssertEqual(library.vms.map(\.slug), [config.slug])
        XCTAssertEqual(library.selectedID, importSelection)
        XCTAssertTrue(library.proMode)
    }

    func testReloadKeepsEmptyLibraryImportDestination() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false)
        library.selectedID = importSelection

        library.reload()

        XCTAssertTrue(library.vms.isEmpty)
        XCTAssertEqual(library.selectedID, importSelection)
    }
}
