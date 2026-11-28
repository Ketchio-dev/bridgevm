#if DEBUG && BRIDGEVM_APP_UI_HOST
import SwiftUI
import XCTest
@testable import BridgeVMControl

/// The re-specified overview wait against the real views (as values, no window):
/// the real import action, then the real sidebar overview button action, both
/// reached through their SwiftUI values. Then the host's own row matcher
/// against owned accessibility elements.
@MainActor
final class AppUIHostOverviewStateContractTests: XCTestCase {
    func testWaitFindsEachCurrentRowOnceInTheImportFormOverview() throws {
        let base = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("bridgevm-ui-host-overview-state-" + UUID().uuidString, isDirectory: true)
        let output = base.appendingPathComponent("app-ui-private/host-observations", isDirectory: true)
        let root = output.appendingPathComponent("fixture-library", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        addTeardownBlock { try? FileManager.default.removeItem(at: base) }
        let capture = try AppUIHostCapture(output: output)
        let library = AppUIHost.makeLibrary(root: root, capture: capture)
        var window = LibraryWindowPresentation()
        let binding = Binding(get: { window }, set: { window = $0 })
        let welcome = FirstRunView(library: library).welcome(in: binding, diskPath: .constant(""))
        let sidebar = LibrarySidebar(library: library)
        let overview = overviewButtonContents(in: sidebar.body).flatMap { closures(in: $0) }
        XCTAssertEqual(overview.count, 1, "exactly one bridgevm.library.overview button action")
        welcome.importAction()
        XCTAssertTrue(window.showsImportDraft)
        XCTAssertNil(library.selectedID)
        sidebar.showOverview(in: binding)
        XCTAssertFalse(window.showsImportDraft)
        XCTAssertNil(library.selectedID)

        welcome.importAction()
        AppUIHost.seedOverview(library, root: root, work: capture.work)
        sidebar.showOverview(in: binding)
        XCTAssertTrue(AppUIHostOverviewState.reached(library))
        XCTAssertFalse(window.showsImportDraft)
        let detail = try XCTUnwrap(values(LibraryDetailView.self, in: LibraryDashboardRoute(library: library).body).first)
        let emptyDetail = try XCTUnwrap(values(LibraryEmptyState.self, in: detail.body).first)
        XCTAssertTrue(values(LibraryOverviewView.self, in: emptyDetail.body).isEmpty,
                      "the detail must not repeat the adjacent searchable VM list")
        XCTAssertEqual(values(LibraryWorkspaceList.self, in: LibraryOverviewView(library: library, query: .constant("")).body).count, 1)
        XCTAssertEqual(capture.work.modelCreations["after_seeding"], 0, "leaving a draft must not activate a VM")

        let refused: [(Bool, String?)] = [(true, nil), (false, "ui-amber"), (false, LibraryModel.firstRunImportSelectionID)]
        for (proMode, selection) in refused {
            library.proMode = proMode
            library.selectedID = selection
            XCTAssertFalse(AppUIHostOverviewState.reached(library))
        }
        library.selectedID = LibraryModel.firstRunImportSelectionID
        _ = library.firstRunImport.begin()
        overview.forEach { $0() }
        XCTAssertEqual(library.selectedID, LibraryModel.firstRunImportSelectionID, "keep active import progress visible")
        library.firstRunImport.returnToInputs()
        library.selectedID = nil
        overview.forEach { $0() }
        XCTAssertEqual(library.selectedID, "ui-amber", "normal overview outside a draft still selects the first VM")

        let content = NSView()
        func element(_ label: String, role: NSAccessibility.Role = .button) -> NSAccessibilityElement {
            let element = NSAccessibilityElement()
            element.setAccessibilityRole(role)
            element.setAccessibilityLabel(label)
            return element
        }
        let amber = element(AppUIDriverOverviewRows.amber), indigo = element(AppUIDriverOverviewRows.indigo)
        content.setAccessibilityChildren([amber, element("Amber 작업", role: .menuButton), indigo,
                                          element("Indigo, HVF, stopped"), element(AppUIDriverOverviewRows.amber, role: .staticText)])
        XCTAssertTrue(try AppUIHostAccessibility.hasRow(AppUIDriverOverviewRows.amber, in: content))
        XCTAssertTrue(try AppUIHostAccessibility.hasRow(AppUIDriverOverviewRows.indigo, in: content))
        content.setAccessibilityChildren([element("Amber, HVF, stopped"), indigo])
        XCTAssertFalse(try AppUIHostAccessibility.hasRow(AppUIDriverOverviewRows.amber, in: content), "stale card label")
        content.setAccessibilityChildren([amber, indigo, element(AppUIDriverOverviewRows.amber)])
        XCTAssertThrowsError(try AppUIHostAccessibility.hasRow(AppUIDriverOverviewRows.amber, in: content), "two lists")
        XCTAssertEqual(AppUIDriverOverviewRows.amber, "Amber, Stopped, 4 CPUs · 4 GB memory")
        XCTAssertEqual(AppUIDriverOverviewRows.indigo, "Indigo, Stopped, 4 CPUs · 8 GB memory")
    }

    private func values<T>(_ type: T.Type, in value: Any) -> [T] {
        if let typed = value as? T { return [typed] }
        let mirror = Mirror(reflecting: value)
        guard mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { values(type, in: $0.value) }
    }
    private func overviewButtonContents(in value: Any, depth: Int = 0) -> [Any] {
        let mirror = Mirror(reflecting: value)
        if String(describing: Swift.type(of: value)).hasPrefix("ModifiedContent<"),
           let modifier = mirror.children.first(where: { $0.label == "modifier" })?.value,
           let content = mirror.children.first(where: { $0.label == "content" })?.value {
            var text = ""; dump(modifier, to: &text)
            if text.contains("\"bridgevm.library.overview\"") { return [content] }
        }
        guard depth < 64, mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { overviewButtonContents(in: $0.value, depth: depth + 1) }
    }
    private func closures(in value: Any, depth: Int = 0) -> [@MainActor () -> Void] {
        if let action = value as? @MainActor () -> Void { return [action] }
        let mirror = Mirror(reflecting: value)
        guard depth < 64, mirror.displayStyle != .class else { return [] }
        return mirror.children.flatMap { closures(in: $0.value, depth: depth + 1) }
    }
}
#endif
