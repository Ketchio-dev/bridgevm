import AppKit
import XCTest
@testable import BridgeVMControl

/// The d6-app-ui (XCTest) tier's copy of the overview spec equals the host's
/// re-spec: the same exact row labels, each found once, and the same state.
@MainActor
final class HvfAppUIOverviewContractTests: XCTestCase {
    func testOverviewSpecFindsEachCurrentRowOnceInTheImportFormOverview() throws {
        XCTAssertEqual(HvfAppUIOverview.amber, "Amber, 정지됨, 4 vCPU  ·  4 GiB RAM  ·  HVF")
        XCTAssertEqual(HvfAppUIOverview.indigo, "Indigo, 정지됨, 4 vCPU  ·  8 GiB RAM  ·  HVF")
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = LibraryModel(rootURL: root, migrateLegacy: false, startsModelsAutomatically: false)
        library.selectedID = LibraryModel.firstRunImportSelectionID
        XCTAssertTrue(HvfAppUIOverview.reached(library), "Pro mode off, the import selection kept")
        let refused: [(Bool, String?)] = [(true, nil), (false, nil), (false, "ui-amber"),
                                          (true, LibraryModel.firstRunImportSelectionID)]
        for (proMode, selection) in refused {
            library.proMode = proMode
            library.selectedID = selection
            XCTAssertFalse(HvfAppUIOverview.reached(library))
        }

        let content = NSView()
        func element(_ label: String, role: NSAccessibility.Role = .button) -> NSAccessibilityElement {
            let element = NSAccessibilityElement()
            element.setAccessibilityRole(role)
            element.setAccessibilityLabel(label)
            return element
        }
        let amber = element(HvfAppUIOverview.amber), indigo = element(HvfAppUIOverview.indigo)
        content.setAccessibilityChildren([amber, element("Amber 작업", role: .menuButton), indigo,
                                          element("Indigo, HVF, stopped"), element(HvfAppUIOverview.amber, role: .staticText)])
        XCTAssertTrue(try HvfAppUIAccessibility.hasRow(HvfAppUIOverview.amber, in: content))
        XCTAssertTrue(try HvfAppUIAccessibility.hasRow(HvfAppUIOverview.indigo, in: content))
        content.setAccessibilityChildren([element("Amber, HVF, stopped"), indigo])
        XCTAssertFalse(try HvfAppUIAccessibility.hasRow(HvfAppUIOverview.amber, in: content), "stale card label")
        content.setAccessibilityChildren([amber, indigo, element(HvfAppUIOverview.amber)])
        XCTAssertThrowsError(try HvfAppUIAccessibility.hasRow(HvfAppUIOverview.amber, in: content), "two lists")
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }
}
