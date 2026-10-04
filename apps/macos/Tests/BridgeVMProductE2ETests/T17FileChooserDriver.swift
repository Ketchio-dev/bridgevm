import XCTest
@testable import BridgeVMProductE2E

final class T17FileChooserDriver: T17FileChooserDriving {
    var actions: [String] = []
    var panel = false, fieldReady = true, locationCloses = true, openEnabled = true
    var panelCloses = true, inventoryFails = false, pathRejected = false
    var panelDelay = 0
    var path: String?
    var returnedPath: String?
    func open() { actions.append("open"); panel = true }
    func panelIsPresent() throws -> Bool {
        if inventoryFails { throw T17FileChooser.failure("unreadable inventory") }
        if panel && panelDelay > 0 { panelDelay -= 1; return false }
        return panel
    }
    func showLocationField() { actions.append("show-location") }
    func locationFieldIsReady() -> Bool { fieldReady }
    func setLocation(_ value: String) throws {
        actions.append("set-location")
        if pathRejected { throw T17FileChooser.failure("rejected path") }
        path = value
    }
    func acceptLocation() { actions.append("accept-location") }
    func locationFieldIsAbsent() -> Bool { locationCloses }
    func acceptSelectionIfReady() -> Bool {
        guard openEnabled else { return false }; actions.append("accept-selection")
        if panelCloses { panel = false }; return true
    }
    func selectedPath() -> String? { returnedPath ?? path }
}
