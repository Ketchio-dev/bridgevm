import XCTest
@testable import BridgeVMProductE2E

final class T17RuntimeIntegrationSetupTests: XCTestCase {
    func testSetsIntegrationsInTheSettingsSheetAndClosesItBeforeStart() throws {
        let ui = T17SettingsSheetProductUI()
        try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", ui: ui)
        try ui.press("bridgevm.windows.runtime.start", timeout: 20)
        XCTAssertEqual(ui.events, [
            "press bridgevm.dashboard.back", "press bridgevm.runtime.settings.open", "wait bridgevm.runtime.settings",
            "toggle bridgevm.runtime.clipboard true", "toggle bridgevm.runtime.network true",
            "toggle bridgevm.runtime.share.enabled true",
            "choose /private/lane/share bridgevm.runtime.share.host.choose", "text bridgevm.runtime.share.guest",
            "press bridgevm.runtime.settings.done", "press bridgevm.dashboard.advanced", "wait bridgevm.windows.runtime.view",
            "press bridgevm.windows.runtime.start",
        ])
    }

    func testChooserFailureIsRetained() {
        let ui = T17SettingsSheetProductUI(); ui.chooseError = T17Blocker(code: "input-selection-failed", detail: "measured")
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "input-selection-failed")
        }
    }

    func testChangedGuestDefaultFailsClosed() {
        let ui = T17SettingsSheetProductUI(); ui.guestShare = "D:\\unexpected"
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "ui-element-missing")
        }
    }
}
