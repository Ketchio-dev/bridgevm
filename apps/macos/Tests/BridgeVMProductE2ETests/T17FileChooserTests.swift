import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserTests: XCTestCase {
    private let path = "/private/test media/installer.iso"

    private func run(_ driver: Driver, path: String? = nil) throws {
        var clock: TimeInterval = 0
        try T17FileChooser.choose(path: path ?? self.path, timeout: 1, driver: driver,
                                  now: { clock }, pause: { clock += 0.1 })
    }

    func testDelayedPanelMustReachEveryStageAndExactPath() throws {
        let driver = Driver()
        driver.panelDelay = 3
        try run(driver)
        XCTAssertEqual(driver.actions, ["open", "show-location", "set-location", "accept-location", "accept-selection"])
        XCTAssertEqual(driver.path, path)
    }

    func testExistingPanelRefusesToOpenAnother() {
        let driver = Driver(); driver.panel = true
        XCTAssertThrowsError(try run(driver))
        XCTAssertTrue(driver.actions.isEmpty)
    }

    func testMissingLocationFieldDoesNotTypeOrContinue() {
        let driver = Driver(); driver.fieldReady = false
        XCTAssertThrowsError(try run(driver))
        XCTAssertEqual(driver.actions, ["open", "show-location"])
    }

    func testGoToSheetMustCloseBeforeOpenIsPressed() {
        let driver = Driver(); driver.locationCloses = false
        XCTAssertThrowsError(try run(driver))
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    func testDisabledOpenIsNeverPressed() {
        let driver = Driver(); driver.openEnabled = false
        XCTAssertThrowsError(try run(driver))
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    func testDismissalWithoutExactSelectionIsNotSuccess() {
        for selected in ["", "installer.iso", "/different/installer.iso"] {
            let driver = Driver(); driver.returnedPath = selected
            XCTAssertThrowsError(try run(driver))
        }
    }

    /// r96: the manifest named a symlinked path and the panel reported the resolved one.
    func testSymlinkedRequestPathAcceptsTheSameFileButNotAnother() throws {
        let files = FileManager.default
        let root = files.temporaryDirectory.appendingPathComponent("t17-chooser-\(UUID().uuidString)")
        defer { try? files.removeItem(at: root) }
        let media = root.appendingPathComponent("media"), link = root.appendingPathComponent("link")
        try files.createDirectory(at: media, withIntermediateDirectories: true)
        try files.createSymbolicLink(at: link, withDestinationURL: media)
        try Data("iso".utf8).write(to: media.appendingPathComponent("installer.iso"))
        try Data("iso".utf8).write(to: media.appendingPathComponent("other.iso"))
        let requested = link.appendingPathComponent("installer.iso").path

        let driver = Driver(); driver.returnedPath = media.appendingPathComponent("installer.iso").path
        XCTAssertNotEqual(driver.returnedPath, requested)
        try run(driver, path: requested)
        for selected in [media.appendingPathComponent("other.iso").path, "media/installer.iso"] {
            let refused = Driver(); refused.returnedPath = selected
            XCTAssertThrowsError(try run(refused, path: requested))
        }
    }

    func testExactSelectionWithOpenPanelIsNotSuccess() {
        let driver = Driver(); driver.panelCloses = false
        XCTAssertThrowsError(try run(driver))
    }

    func testUnreadableWindowInventoryIsNotAbsence() {
        let driver = Driver(); driver.inventoryFails = true
        XCTAssertThrowsError(try run(driver))
        XCTAssertTrue(driver.actions.isEmpty)
    }

    func testRejectedPathStopsBeforeConfirmation() {
        let driver = Driver(); driver.pathRejected = true
        XCTAssertThrowsError(try run(driver))
        XCTAssertFalse(driver.actions.contains("accept-location"))
        XCTAssertFalse(driver.actions.contains("accept-selection"))
    }

    func testRelativePathIsRejectedBeforeAnyAction() {
        let driver = Driver()
        XCTAssertThrowsError(try run(driver, path: "installer.iso"))
        XCTAssertTrue(driver.actions.isEmpty)
    }

    private typealias Driver = T17FileChooserDriver
}
