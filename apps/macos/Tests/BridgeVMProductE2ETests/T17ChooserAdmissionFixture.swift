import ApplicationServices
@testable import BridgeVMProductE2E

final class T17ChooserAdmissionFixture: T17FileChooserDriving {
    var clock = 0.0, enabledDelay = 0.0, pressDelay = 0.0, acceptDelay = 0.0
    var openDeadline: Double?
    var openClock: (() -> Double)?
    var panel = false, presses = 0, panelReads = 0, selectedReads = 0
    var openResult = AXError.success
    var actions: [String] = []
    func open() throws {
        actions.append("open")
        guard let openDeadline else { panel = true; return }
        _ = try T17ChooserOpenAction.perform(identifier: "fixture", deadline: openDeadline,
            enabled: { self.clock += self.enabledDelay; return true },
            press: { self.presses += 1; self.clock += self.pressDelay; self.panel = true; return self.openResult },
            now: openClock ?? { self.clock }, pause: {})
    }
    func panelIsPresent() -> Bool { panelReads += 1; return panel }
    func showLocationField() { actions.append("location") }
    func locationFieldIsReady() -> Bool { true }
    func setLocation(_ path: String) { actions.append("write") }
    func acceptLocation() { actions.append("accept-location") }
    func locationFieldIsAbsent() -> Bool { true }
    func selectionIsReady() -> Bool { true }
    func acceptSelection() { actions.append("accept-selection"); clock += acceptDelay; panel = false }
    func selectedPath() -> String? { selectedReads += 1; return "/fixture/share" }
}
