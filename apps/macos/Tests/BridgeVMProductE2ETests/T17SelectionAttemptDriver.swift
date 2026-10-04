import ApplicationServices
@testable import BridgeVMProductE2E

final class T17SelectionAttemptDriver: T17FileChooserDriving {
    var clock = 0.0, relationshipCost = 0.0
    var roots = 0, selectedReads = 0
    var relationshipReads: [Int] = [], enabledReads: [Int] = [], pressed: [Int] = []
    var edges = [0: [1], 1: [2], 2: []]
    var roles = [0: "AXApplication", 1: "AXWindow", 2: "AXButton"]
    var identifiers = [1: "open-panel", 2: "OKButton"]
    var panel = false
    var enabled = true, pressResult = AXError.success
    var enabledRead: ((Int) throws -> Bool?)?
    var afterPause: (() -> Void)?
    var pressError: T17Blocker?, pressReturnTime: Double?
    var failureContext: String { "owned selection fixture" }
    var budget: T17ChooserNativeBudget { .init(deadline: 20, now: { self.clock }) }

    func choose() throws {
        try T17FileChooser.choose(path: "/fixture/share", timeout: 20, driver: self,
            now: { self.clock }, pause: { self.clock += 0.05; self.afterPause?() })
    }
    func lookup() throws -> Int? {
        try T17FileChooserSelectionLookup.read(budget: budget,
            root: { self.roots += 1; return 0 }, related: { node in
                try self.budget.read(.relationship) {
                    self.relationshipReads.append(node); self.clock += self.relationshipCost
                    return self.edges[node] ?? []
                }
            }, hash: { _ in 1 }, same: ==,
            role: { self.roles[$0] }, identifier: { self.identifiers[$0] }, pause: {})
    }
    func open() { panel = true }
    func panelIsPresent() -> Bool { panel }
    func showLocationField() {}
    func locationFieldIsReady() -> Bool { true }
    func setLocation(_ path: String) {}
    func acceptLocation() {}
    func locationFieldIsAbsent() -> Bool { true }
    func acceptSelectionIfReady() throws -> Bool {
        try T17ChooserSelectionAction.perform(budget: budget, lookup: lookup,
            enabled: { node in
                self.enabledReads.append(node)
                if let read = self.enabledRead { return try read(node) }
                return self.enabled
            }, press: { node in
                let result = try self.budget.input(.selectionPress) {
                    self.pressed.append(node); self.panel = false
                    if let time = self.pressReturnTime { self.clock = time }
                    return self.pressResult
                }
                if let error = self.pressError { throw error }
                guard result == .success else { throw T17FileChooser.failure("file chooser AXPress failed; ax_error=\(result.rawValue)") }
            })
    }
    func selectedPath() -> String? { selectedReads += 1; return "/fixture/share" }
}
