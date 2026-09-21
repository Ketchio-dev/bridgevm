import XCTest
@testable import BridgeVMProductE2E

final class T17RuntimeIntegrationSetupTests: XCTestCase {
    func testUsesProductTogglesAndDirectoryChooserWithoutTextInjection() throws {
        let ui = RecordingUI()
        try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", ui: ui)
        XCTAssertEqual(ui.events, [
            .toggle(true, "bridgevm.runtime.clipboard", 10),
            .toggle(true, "bridgevm.runtime.network", 10),
            .toggle(true, "bridgevm.runtime.share.enabled", 10),
            .choose("/private/lane/share", "bridgevm.runtime.share.host.choose", 20),
            .text("bridgevm.runtime.share.guest", 10),
        ])
    }

    func testChooserFailureIsRetained() {
        let ui = RecordingUI(); ui.chooseError = T17Blocker(code: "input-selection-failed", detail: "measured")
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "input-selection-failed")
        }
    }

    func testChangedGuestDefaultFailsClosed() {
        let ui = RecordingUI(); ui.textValue = "D:\\unexpected"
        XCTAssertThrowsError(try T17RuntimeIntegrationSetup.apply(sharePath: "/private/lane/share", ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.code, "ui-element-missing")
        }
    }
}

private final class RecordingUI: T17UIControlling {
    enum Event: Equatable { case toggle(Bool, String, TimeInterval); case choose(String, String, TimeInterval); case text(String, TimeInterval) }
    var events: [Event] = []; var chooseError: Error?; var textValue = "C:\\bridgevm-share"
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {
        events.append(.toggle(enabled, identifier, timeout))
    }
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {
        events.append(.choose(path, identifier, timeout)); if let chooseError { throw chooseError }
    }
    func press(_ identifier: String, timeout: TimeInterval) throws {}
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws {}
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {}
    func text(_ identifier: String, timeout: TimeInterval) throws -> String {
        events.append(.text(identifier, timeout)); return textValue
    }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func clickSecondaryWindow(timeout: TimeInterval) throws {}
    func textSnapshot() -> [String] { [] }
}
