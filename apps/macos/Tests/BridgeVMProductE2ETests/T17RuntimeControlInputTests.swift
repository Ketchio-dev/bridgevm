import XCTest
@testable import BridgeVMProductE2E

final class T17RuntimeControlInputTests: XCTestCase {
    func testCollapsedDiagnosticsAreOpenedBeforeTheCommandIsEntered() throws {
        let ui = ControlUI(inputVisible: false)
        try T17RuntimeControlInput.enter("CLIPGET", ui: ui)
        XCTAssertEqual(ui.events, [
            "wait bridgevm.runtime.ctl.input 1.0", "press bridgevm.runtime.diagnostics.toggle 10.0",
            "set bridgevm.runtime.ctl.input CLIPGET", "press bridgevm.runtime.ctl.send 10.0",
        ])
    }

    func testOpenDiagnosticsAreNotToggledClosed() throws {
        let ui = ControlUI(inputVisible: true)
        try T17RuntimeControlInput.enter("CLIPGET", ui: ui)
        XCTAssertEqual(ui.events, [
            "wait bridgevm.runtime.ctl.input 1.0",
            "set bridgevm.runtime.ctl.input CLIPGET", "press bridgevm.runtime.ctl.send 10.0",
        ])
    }

    func testMissingOpenerFailsBeforeAnyCommandIsEntered() {
        let ui = ControlUI(inputVisible: false); ui.pressError = T17Blocker(code: "ui-element-missing", detail: "opener")
        XCTAssertThrowsError(try T17RuntimeControlInput.enter("CLIPGET", ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "opener")
        }
        XCTAssertFalse(ui.events.contains { $0.hasPrefix("set ") })
    }
}

private final class ControlUI: T17UIControlling {
    var events: [String] = []; var inputVisible: Bool; var pressError: Error?
    init(inputVisible: Bool) { self.inputVisible = inputVisible }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {
        events.append("wait \(identifier) \(timeout)")
        if !inputVisible { throw T17Blocker(code: "ui-element-missing", detail: "absent") }
    }
    func press(_ identifier: String, timeout: TimeInterval) throws {
        events.append("press \(identifier) \(timeout)")
        if let pressError { throw pressError }
    }
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws {
        events.append("set \(identifier) \(value)")
    }
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {}
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {}
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { "" }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func clickSecondaryWindow(timeout: TimeInterval) throws {}
    func textSnapshot() -> [String] { [] }
}
