import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserActionDeadlineDriver: T17FileChooserDriving {
    var time = 0.0, slow = "", panel = false, fieldReads = 0
    var actions: [String] = []
    var failureContext: String { "ax=retained" }
    private func delay(_ stage: String) { if slow == stage { time = 1 } }
    private func act(_ stage: String) { actions.append(stage); delay(stage) }
    func open() { act("open"); panel = true }
    func panelIsPresent() -> Bool {
        if !panel { delay("initial-panel-check") }
        return panel
    }
    func showLocationField() { act("show-location") }
    func locationFieldIsReady() -> Bool { fieldReads += 1; return true }
    func setLocation(_ path: String) { act("set-location") }
    func acceptLocation() { act("accept-location") }
    func locationFieldIsAbsent() -> Bool { true }
    func acceptSelectionIfReady() -> Bool {
        delay("selection-lookup"); guard time < 1 else { return false }
        act("accept-selection"); panel = false; return true
    }
    func selectedPath() -> String? { "/private/fixture.iso" }
}
