import AppKit
import ApplicationServices

final class T17FileChooserAX: T17FileChooserDriving {
    private let pid: pid_t
    private let application: AXUIElement
    private let selection: T17ChooserSelectionTarget
    private let io: T17FileChooserAXIO
    private let openControl: () throws -> AXError
    private var openResult: AXError?
    private var panel: AXUIElement?
    private var keyContext = "key not sent"
    private var activationSucceeded: Bool?
    private let predicates = T17FileChooserPredicateDiagnostics()

    var failureContext: String {
        "open_ax_result=\(openResult.map { String($0.rawValue) } ?? "not-recorded"); " + predicates.context(activation: activationSucceeded, key: keyContext, timeout: T17FileChooserDiagnostics.snapshot(pid: pid), tree: T17FileChooserTreeDiagnostics.snapshot(application))
    }

    init(pid: pid_t, selection: T17ChooserSelectionTarget, io: T17FileChooserAXIO, openControl: @escaping () throws -> AXError) {
        self.pid = pid
        self.application = AXUIElementCreateApplication(pid)
        self.selection = selection
        self.io = io
        self.openControl = openControl
    }

    func open() throws { openResult = try openControl() }
    func panelIsPresent() throws -> Bool { try currentPanel() != nil }
    private func currentPanel() throws -> AXUIElement? {
        panel = try io.snapshot(root: { AXUIElementCreateApplication(self.pid) }, nodes: nodes) { candidates in
            try T17RoleFirstIdentity.find(
                in: candidates,
                id: "open-panel",
                roles: [kAXWindowRole, kAXSheetRole, "AXDialog"],
                role: { try self.attribute($0, kAXRoleAttribute) as? String },
                identifier: { try self.attribute($0, kAXIdentifierAttribute) as? String },
                same: { CFEqual($0, $1) }
            )
        }
        return panel
    }
    func showLocationField() throws {
        guard let panel else { throw T17FileChooser.failure("file chooser was absent") }
        // Activation is recovery, not proof that the chooser can accept input.
        activationSucceeded = io.activate(pid: pid).succeeded
        try io.budget.check(.activation, boundary: .returned)
        try action(panel, kAXRaiseAction)
        try key(5, flags: [.maskCommand, .maskShift])
    }
    func locationFieldIsReady() throws -> Bool { try locationField() != nil }
    func setLocation(_ path: String) throws {
        guard let field = try locationField() else { throw T17FileChooser.failure("Go To field disappeared") }
        let result = try io.setPath(path, field: field)
        guard result == .success,
              try attribute(field, kAXValueAttribute) as? String == path else {
            throw T17FileChooser.failure("Go To field rejected the exact path; ax_error=\(result.rawValue)")
        }
    }
    func acceptLocation() throws { try key(36) }
    func locationFieldIsAbsent() throws -> Bool { try locationSheet() == nil }

    func selectionIsReady() throws -> Bool {
        guard let button = try openButton() else { return false }
        return try (attribute(button, kAXEnabledAttribute) as? NSNumber)?.boolValue == true
    }
    func acceptSelection() throws {
        try T17ChooserSelectionAction.perform(budget: io.budget, lookup: openButton,
            enabled: { try (self.attribute($0, kAXEnabledAttribute) as? NSNumber)?.boolValue },
            press: { try self.action($0, kAXPressAction) })
    }
    func selectedPath() throws -> String? {
        try selection.read(in: nodes(application),
            role: { try attribute($0, kAXRoleAttribute) as? String },
            identifier: { try attribute($0, kAXIdentifierAttribute) as? String },
            value: { try attribute($0, kAXValueAttribute) as? String },
            same: { CFEqual($0, $1) })
    }

    private func locationSheet() throws -> AXUIElement? {
        let current = panel
        predicates.beginOwner(panelCached: current != nil)
        guard let current else { return nil }
        return try semantic(in: current, id: "GoToWindow", roles: [kAXSheetRole], walk: io.sheetCandidates)
    }
    private func locationField() throws -> AXUIElement? {
        guard let sheet = try locationSheet() else { return nil }
        return try semantic(in: sheet, id: "PathTextField", roles: [kAXTextFieldRole, kAXComboBoxRole])
    }
    private func semantic(in root: AXUIElement, id: String, roles: Set<String>, walk: ((AXUIElement) throws -> [AXUIElement])? = nil) throws -> AXUIElement? {
        try io.snapshot(root: { root }, nodes: walk ?? { try self.nodes($0) }) { candidates in
            if let identified = try predicates.find(in: { candidates }, id: id, roles: roles, metadata: {
                (try self.attribute($0, kAXIdentifierAttribute) as? String,
                 try self.attribute($0, kAXRoleAttribute) as? String)
            }, same: { CFEqual($0, $1) }) { return identified }
            return try T17FileChooserSemanticIdentity.find(in: candidates, id: id, roles: roles, metadata: {
                (try self.attribute($0, kAXIdentifierAttribute) as? String,
                 try self.attribute($0, kAXRoleAttribute) as? String)
            }, same: { CFEqual($0, $1) })
        }
    }

    private func openButton() throws -> AXUIElement? {
        try T17FileChooserSelectionAX.read(pid: pid, io: io)
    }

    private func key(_ code: CGKeyCode, flags: CGEventFlags = []) throws {
        keyContext = try io.key(pid: pid, code: code, flags: flags)
    }

    private func action(_ element: AXUIElement, _ name: String) throws {
        try io.action(element, name)
    }

    private func attribute(_ element: AXUIElement, _ name: String) throws -> AnyObject? {
        try io.attribute(element, name)
    }
    private func nodes(_ root: AXUIElement) throws -> [AXUIElement] {
        try io.nodes(root)
    }
}
