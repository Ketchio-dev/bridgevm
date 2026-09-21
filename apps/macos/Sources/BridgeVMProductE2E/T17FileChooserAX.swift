import AppKit
import ApplicationServices

final class T17FileChooserAX: T17FileChooserDriving {
    private let pid: pid_t
    private let application: AXUIElement
    private let identifier: String
    private let openControl: () throws -> Void
    private var panel: AXUIElement?
    private var keyContext = "key not sent"
    private var activationSucceeded: Bool?
    private let predicates = T17FileChooserPredicateDiagnostics()

    var failureContext: String {
        predicates.context(activation: activationSucceeded, key: keyContext, timeout: T17FileChooserDiagnostics.snapshot(pid: pid), tree: T17FileChooserTreeDiagnostics.snapshot(application))
    }

    init(pid: pid_t, identifier: String, openControl: @escaping () throws -> Void) {
        self.pid = pid
        self.application = AXUIElementCreateApplication(pid)
        self.identifier = identifier
        self.openControl = openControl
    }

    func open() throws { try openControl() }
    func panelIsPresent() throws -> Bool { try currentPanel() != nil }
    private func currentPanel() throws -> AXUIElement? {
        panel = try T17FileChooserAXTree.applicationSnapshot(pid: pid) { candidates in
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
        activationSucceeded = T17Activation.bringToFront(pid: pid)
        try action(panel, kAXRaiseAction)
        try key(5, flags: [.maskCommand, .maskShift])
    }
    func locationFieldIsReady() throws -> Bool { try locationField() != nil }
    func setLocation(_ path: String) throws {
        guard let field = try locationField() else { throw T17FileChooser.failure("Go To field disappeared") }
        let result = AXUIElementSetAttributeValue(field, kAXValueAttribute as CFString, path as CFString)
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
        guard let button = try openButton(),
              try (attribute(button, kAXEnabledAttribute) as? NSNumber)?.boolValue == true else {
            throw T17FileChooser.failure("Open button was not enabled at confirmation")
        }
        try action(button, kAXPressAction)
    }
    func selectedPath() throws -> String? {
        let expected = identifier + ".selection"
        guard let node = try nodes(application).first(where: {
            try attribute($0, kAXIdentifierAttribute) as? String == expected
        }) else { return nil }
        return try attribute(node, kAXValueAttribute) as? String
    }

    private func locationSheet() throws -> AXUIElement? {
        let current = panel
        predicates.beginOwner(panelCached: current != nil)
        guard let current else { return nil }
        return try semantic(in: current, id: "GoToWindow", roles: [kAXSheetRole])
    }
    private func locationField() throws -> AXUIElement? {
        guard let sheet = try locationSheet() else { return nil }
        return try semantic(in: sheet, id: "PathTextField", roles: [kAXTextFieldRole, kAXComboBoxRole])
    }
    private func semantic(in root: AXUIElement, id: String, roles: Set<String>) throws -> AXUIElement? {
        try T17FileChooserOwnedSnapshot.read(owner: root, nodes: nodes) { candidates in
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
        guard let current = try currentPanel() else { return nil }
        return try nodes(current).first { try attribute($0, kAXIdentifierAttribute) as? String == "OKButton" }
    }

    private func key(_ code: CGKeyCode, flags: CGEventFlags = []) throws {
        keyContext = try T17FileChooserDiagnostics.post(pid: pid, code: code, flags: flags)
    }

    private func action(_ element: AXUIElement, _ name: String) throws {
        let result = AXUIElementPerformAction(element, name as CFString)
        guard result == .success else {
            throw T17FileChooser.failure("file chooser \(name) failed; ax_error=\(result.rawValue)")
        }
    }

    private func attribute(_ element: AXUIElement, _ name: String) throws -> AnyObject? {
        var rawNames: CFArray?
        let result = AXUIElementCopyAttributeNames(element, &rawNames)
        guard result == .success else {
            throw T17FileChooser.failure("file chooser AXAttributeNames read failed; ax_error=\(result.rawValue)")
        }
        guard let names = rawNames as? [String] else { throw T17FileChooser.failure("file chooser AXAttributeNames was invalid") }
        return try T17SupportedAttribute.read(name, advertised: { names },
                                              value: { try T17FileChooserAXTree.attribute(element, name) })
    }
    private func nodes(_ root: AXUIElement) throws -> [AXUIElement] {
        try T17FileChooserAXTree.nodes(root)
    }
}
