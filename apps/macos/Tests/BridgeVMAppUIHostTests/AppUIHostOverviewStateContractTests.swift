#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit
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
        try XCTUnwrap(values(FirstRunWelcomeView.self, in: FirstRunView(library: library).body).first).importAction()
        AppUIHost.seedOverview(library, root: root, work: capture.work)
        let overview = overviewButtonContents(in: LibrarySidebar(library: library).body).flatMap { closures(in: $0) }
        XCTAssertEqual(overview.count, 1, "exactly one bridgevm.library.overview button action")
        overview.forEach { $0() }
        XCTAssertTrue(AppUIHostOverviewState.reached(library), "the scenario's state after import, seeding and overview")
        XCTAssertFalse(library.proMode || library.selectedID == nil, "the d1f17b2d wait (Pro mode, no selection) is unmet")

        let detail = try XCTUnwrap(values(LibraryDetailView.self, in: LibraryDashboardRoute(library: library).body).first)
        XCTAssertEqual(values(FirstRunView.self, in: detail.body).count, 1, "the detail pane keeps the import form")
        XCTAssertTrue(values(FleetTableView.self, in: detail.body).isEmpty, "no second workspace list")
        XCTAssertEqual(values(LibraryWorkspaceList.self, in: LibraryOverviewView(library: library).body).count, 1)
        library.proMode = true
        library.selectedID = nil
        let stale = try XCTUnwrap(values(LibraryDetailView.self, in: LibraryDashboardRoute(library: library).body).first)
        XCTAssertEqual(values(FleetTableView.self, in: stale.body).count, 1, "the old expectation renders a second list")
        let refused: [(Bool, String?)] = [(true, nil), (false, nil), (false, "ui-amber"),
                                          (true, LibraryModel.firstRunImportSelectionID)]
        for (proMode, selection) in refused {
            library.proMode = proMode
            library.selectedID = selection
            XCTAssertFalse(AppUIHostOverviewState.reached(library))
        }
        XCTAssertEqual(capture.work.modelCreations["after_seeding"], 0, "routing alone requests no row model")
        library.proMode = true
        library.selectedID = LibraryModel.firstRunImportSelectionID
        overview.forEach { $0() }
        XCTAssertTrue(AppUIHostOverviewState.reached(library), "the button leaves Pro mode and keeps the import selection")
        library.selectedID = nil
        overview.forEach { $0() }
        XCTAssertEqual(library.selectedID, "ui-amber", "with seeded rows the button never leaves nothing selected")

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
        XCTAssertEqual(AppUIDriverOverviewRows.amber, "Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF")
        XCTAssertEqual(AppUIDriverOverviewRows.indigo, "Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF")
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
