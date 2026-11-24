import XCTest
@testable import BridgeVMProductE2E

final class T17RuntimeIntegrationSetupTests: XCTestCase {
    func testSetsIntegrationsInTheSettingsSheetAndClosesItBeforeStart() throws {
        let ui = T17SettingsSheetProductUI()
        let bundle = try makeBundle(clipboardSyncEnabled: false)
        try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", bundle: bundle, ui: ui)
        try ui.press("bridgevm.windows.runtime.start", timeout: 20)
        XCTAssertEqual(ui.events, [
            "press bridgevm.dashboard.back", "press bridgevm.runtime.settings.open", "wait bridgevm.runtime.settings",
            "toggle bridgevm.runtime.clipboard false", "toggle bridgevm.runtime.network true",
            "toggle bridgevm.runtime.share.enabled true",
            "choose /private/lane/share bridgevm.runtime.share.host.choose", "text bridgevm.runtime.share.guest",
            "press bridgevm.runtime.settings.done", "press bridgevm.dashboard.advanced", "wait bridgevm.windows.runtime.view",
            "press bridgevm.windows.runtime.start",
        ])
    }

    func testRefusesToContinueWhenClipboardSettingWasNotSaved() throws {
        let ui = T17SettingsSheetProductUI()
        let bundle = try makeBundle(clipboardSyncEnabled: true)
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", bundle: bundle, ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "Mac clipboard sync must be disabled in the lane VM registration before Start")
        }
        XCTAssertTrue(ui.events.contains("toggle bridgevm.runtime.clipboard false"))
    }

    func testChooserFailureIsRetained() throws {
        let ui = T17SettingsSheetProductUI(); ui.chooseError = T17Blocker(code: "input-selection-failed", detail: "measured")
        let bundle = try makeBundle(clipboardSyncEnabled: false)
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", bundle: bundle, ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "input-selection-failed")
        }
    }

    func testChangedGuestDefaultFailsClosed() throws {
        let ui = T17SettingsSheetProductUI(); ui.guestShare = "D:\\unexpected"
        let bundle = try makeBundle(clipboardSyncEnabled: false)
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", bundle: bundle, ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "ui-element-missing")
        }
    }

    private func makeBundle(clipboardSyncEnabled: Bool) throws -> URL {
        let vmDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: vmDirectory, withIntermediateDirectories: false)
        addTeardownBlock { try FileManager.default.removeItem(at: vmDirectory) }
        let bundle = vmDirectory.appendingPathComponent("bundle.vmbridge", isDirectory: true)
        let config: [String: Any] = ["id": vmDirectory.lastPathComponent, "bundlePath": bundle.path,
                                     "clipboardSyncEnabled": clipboardSyncEnabled]
        try JSONSerialization.data(withJSONObject: config).write(to: vmDirectory.appendingPathComponent("vm.json"))
        return bundle
    }
}
