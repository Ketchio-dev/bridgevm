import SwiftUI
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

    /// After one failed import, the sidebar's Virtual machines button returns to Get Started and
    /// the next import form starts without the old error.
    func testOverviewAfterFailedImportReturnsToGetStartedWithoutTheOldError() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false)
        var window = LibraryWindowPresentation()
        let binding = Binding(get: { window }, set: { window = $0 })
        FirstRunView(library: library).welcome(in: binding, diskPath: .constant("")).importAction()
        XCTAssertTrue(window.showsImportDraft)
        // What the form's Import button does before it starts the import.
        window.showsImportDraft = false
        library.selectedID = importSelection
        let failure = "Owned import failure"
        let inputs = FirstRunImport.Inputs(displayName: "Owned", diskPath: root.path, memMiB: 4096, cpuCount: 2)
        let returned = await library.importExistingHvfVM(inputs, worker: { _, _, _, _ in .failed(failure) })
        XCTAssertEqual(returned, failure)

        LibrarySidebar(library: library).showOverview(in: binding)

        XCTAssertNil(library.selectedID)
        XCTAssertNil(library.firstRunImportError)
        XCTAssertEqual(values(FirstRunWelcomeView.self, in: FirstRunView(library: library).body).count, 1)
        FirstRunView(library: library).welcome(in: binding, diskPath: .constant("")).importAction()
        XCTAssertTrue(window.showsImportDraft)
        XCTAssertNil(library.firstRunImportError)
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
}
