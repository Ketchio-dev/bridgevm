import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDashboardNavigationTests: XCTestCase {
    func testPaletteProModeReachesFleetAndReturnsToSameSelectedDashboard() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let model = try XCTUnwrap(f.library.selectedModel)
        XCTAssertEqual(f.values(LibraryDashboardDetail.self, in: LibraryDashboardRoute(library: f.library).body).count, 1)
        try f.paletteCommand("Pro 모드 켜기").action()
        XCTAssertTrue(f.library.proMode)
        let route = try f.detailRoute()
        XCTAssertEqual(f.values(FleetTableView.self, in: route.body).count, 1)
        XCTAssertEqual(f.library.selectedID, model.config.slug)
        XCTAssertTrue(f.library.selectedModel === model)
        try f.paletteCommand("Pro 모드 끄기").action()
        XCTAssertFalse(f.library.proMode)
        let restored = try XCTUnwrap(f.values(LibraryDashboardDetail.self,
            in: LibraryDashboardRoute(library: f.library).body).first)
        XCTAssertTrue(restored.model === model)
        XCTAssertEqual(restored.config, model.config)
        XCTAssertFalse(model.running)
        XCTAssertFalse(model.lifecycleBusy)
    }

    func testExplicitImportSelectionKeepsFirstRunAheadOfFleet() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        f.library.selectedID = LibraryModel.firstRunImportSelectionID
        let operation = f.library.firstRunImport.begin()
        f.library.firstRunImport.advance(.copying, operation: operation)
        try f.paletteCommand("Pro 모드 켜기").action()
        let route = try f.detailRoute()
        XCTAssertEqual(f.values(FirstRunView.self, in: route.body).count, 1)
        XCTAssertTrue(f.values(FleetTableView.self, in: route.body).isEmpty)
        XCTAssertTrue(f.library.firstRunImportBusy)
        XCTAssertEqual(f.library.firstRunImport.stage, .copying)
        XCTAssertEqual(f.library.selectedID, LibraryModel.firstRunImportSelectionID)
    }

    func testRetainedControlSelectionKeepsExactHandleAheadOfFleet() throws {
        let f = try LibraryDashboardNavigationFixture()
        defer { f.clean() }
        let model = try XCTUnwrap(f.library.selectedModel)
        model.running = true // An observed active control; no process is launched or adopted.
        f.library.reload() // The synthetic registration is absent, so its active handle is retained.
        let retained = try XCTUnwrap(f.library.selectedRetainedControl)
        try f.paletteCommand("Pro 모드 켜기").action()
        let route = try f.detailRoute()
        let view = try XCTUnwrap(f.values(RetainedControlDetailView.self, in: route.body).first)
        XCTAssertEqual(view.record.id, retained.id)
        XCTAssertTrue(f.values(FleetTableView.self, in: route.body).isEmpty)
        XCTAssertEqual(f.library.selectedID, retained.id)
        XCTAssertFalse(f.library.dismissRetainedControl(retained.id))
        guard case let .generic(_, actual) = view.record.descriptor else { return XCTFail("Expected retained generic control") }
        XCTAssertTrue(actual === model)
        XCTAssertTrue(model.running)
    }
}
