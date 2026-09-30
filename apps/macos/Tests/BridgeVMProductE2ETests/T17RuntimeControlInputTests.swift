import XCTest
@testable import BridgeVMProductE2E

final class T17RuntimeControlInputTests: XCTestCase {
    func testCollapsedDiagnosticsAreOpenedBeforeTheCommandIsEntered() throws {
        let ui = ControlUI(inputVisible: false)
        try T17RuntimeControlInput.enter("CLIPGET", ui: ui)
        XCTAssertEqual(ui.events, [
            "wait bridgevm.runtime.ctl.input 1.0", "expand bridgevm.runtime.diagnostics.toggle 10.0",
            "fill bridgevm.runtime.ctl.input CLIPGET", "press bridgevm.runtime.ctl.send 10.0",
        ])
    }

    func testOpenDiagnosticsAreNotToggledClosed() throws {
        let ui = ControlUI(inputVisible: true)
        try T17RuntimeControlInput.enter("CLIPGET", ui: ui)
        XCTAssertEqual(ui.events, [
            "wait bridgevm.runtime.ctl.input 1.0",
            "fill bridgevm.runtime.ctl.input CLIPGET", "press bridgevm.runtime.ctl.send 10.0",
        ])
    }

    func testMissingOpenerFailsBeforeAnyCommandIsEntered() {
        let ui = ControlUI(inputVisible: false); ui.expandError = T17Blocker(code: "ui-element-missing", detail: "opener")
        XCTAssertThrowsError(try T17RuntimeControlInput.enter("CLIPGET", ui: ui)) {
            XCTAssertEqual(($0 as? T17Blocker)?.detail, "opener")
        }
        XCTAssertFalse(ui.events.contains { $0.hasPrefix("fill ") || $0.hasPrefix("set ") })
    }
}

private final class ControlUI: T17UIControlling {
    var events: [String] = []; var inputVisible: Bool; var expandError: Error?
    init(inputVisible: Bool) { self.inputVisible = inputVisible }
    func waitFor(_ identifier: String, timeout: TimeInterval) throws {
        events.append("wait \(identifier) \(timeout)")
        if !inputVisible { throw T17Blocker(code: "ui-element-missing", detail: "absent") }
    }
    func press(_ identifier: String, timeout: TimeInterval) throws { events.append("press \(identifier) \(timeout)") }
    func expand(_ identifier: String, timeout: TimeInterval) throws {
        events.append("expand \(identifier) \(timeout)"); if let expandError { throw expandError }
    }
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws { events.append("set \(identifier) \(value)") }
    func fill(_ value: String, identifier: String, timeout: TimeInterval) throws { events.append("fill \(identifier) \(value)") }
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws {}
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws {}
    func text(_ identifier: String, timeout: TimeInterval) throws -> String { "" }
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] { [:] }
    func clickSecondaryWindow(timeout: TimeInterval) throws {}
    func textSnapshot() -> [String] { [] }
}
