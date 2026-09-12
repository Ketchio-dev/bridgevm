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

    var failureContext: String {
        "tree{\(T17FileChooserTreeDiagnostics.snapshot(application))}; timeout{\(T17FileChooserDiagnostics.snapshot(pid: pid))}; activation=\(activationSucceeded?.description ?? "unknown"); \(keyContext)"
    }

    init(pid: pid_t, identifier: String, openControl: @escaping () throws -> Void) {
        self.pid = pid
        self.application = AXUIElementCreateApplication(pid)
        self.identifier = identifier
        self.openControl = openControl
    }

    func open() throws { try openControl() }

    func panelIsPresent() throws -> Bool {
        guard let windows = try attribute(application, kAXWindowsAttribute) as? [AXUIElement] else {
            throw T17FileChooser.failure("file chooser window inventory was unavailable")
        }
        let panels = try windows.filter { try attribute($0, kAXIdentifierAttribute) as? String == "open-panel" }
        guard panels.count <= 1 else { throw T17FileChooser.failure("multiple file choosers were open") }
        panel = panels.first
        return panel != nil
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
        guard panel != nil else { return nil }
        return try identified(in: application, id: "GoToWindow", roles: [kAXSheetRole])
    }

    private func locationField() throws -> AXUIElement? {
        guard let sheet = try locationSheet() else { return nil }
        return try identified(in: sheet, id: "PathTextField", roles: [kAXTextFieldRole, kAXComboBoxRole])
    }

    private func identified(in root: AXUIElement, id: String, roles: Set<String>) throws -> AXUIElement? {
        try T17FileChooserIdentity.find(in: nodes(root), id: id, roles: roles, metadata: {
            (try self.attribute($0, kAXIdentifierAttribute) as? String,
             try self.attribute($0, kAXRoleAttribute) as? String)
        }, same: { CFEqual($0, $1) })
    }

    private func openButton() throws -> AXUIElement? {
        guard let panel else { return nil }
        return try nodes(panel).first { try attribute($0, kAXIdentifierAttribute) as? String == "OKButton" }
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
        try T17FileChooserAXTree.attribute(element, name)
    }

    private func nodes(_ root: AXUIElement) throws -> [AXUIElement] {
        try T17FileChooserAXTree.nodes(root)
    }
}
