import XCTest
@testable import BridgeVMProductE2E

final class A9ImportRuntimeNavigationTests: XCTestCase {
    func testFollowsImportedDashboardIntoRuntimeControls() throws {
        let ui = ImportedDashboardUI(runtimeShown: false)
        try A9ImportRuntimeNavigation.openRuntimeControls(ui)
        XCTAssertEqual(ui.presses, ["bridgevm.dashboard.advanced@1200.0"])
        XCTAssertTrue(ui.runtimeShown)
    }

    func testShownRuntimeControlsNeedNoPress() throws {
        let ui = ImportedDashboardUI(runtimeShown: true)
        try A9ImportRuntimeNavigation.openRuntimeControls(ui)
        XCTAssertEqual(ui.presses, [])
    }

    func testMissingDashboardFailsClosed() {
        let ui = ImportedDashboardUI(runtimeShown: false); ui.dashboardShown = false
        XCTAssertThrowsError(try A9ImportRuntimeNavigation.openRuntimeControls(ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "ui-element-missing")
        }
        XCTAssertFalse(ui.runtimeShown)
    }
}

/// The product after an import: the VM dashboard, with runtime controls behind "고급".
private final class ImportedDashboardUI: T17UIControlling {
    var runtimeShown: Bool, dashboardShown = true, presses: [String] = []
    init(runtimeShown: Bool) { self.runtimeShown = runtimeShown }
    private func missing(_ id: String) -> T17Blocker { T17Blocker(code: "ui-element-missing", detail: id) }
    func press(_ identifier: String, timeout: TimeInterval) throws {
        guard identifier == "bridgevm.dashboard.advanced", dashboardShown, !runtimeShown else { throw missing(identifier) }
        presses.append("\(identifier)@\(timeout)"); runtimeShown = true
    }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {
        guard identifier == "bridgevm.windows.runtime.view", runtimeShown else { throw missing(identifier) }
    }
    func expand(_ identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws { throw missing(identifier) }
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { throw missing(identifier) }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval) throws { throw missing("display") }
    func textSnapshot() -> [String] { [] }
}
