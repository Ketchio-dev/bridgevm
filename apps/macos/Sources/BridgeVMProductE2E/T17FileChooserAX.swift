import AppKit
import ApplicationServices

final class T17FileChooserAX: T17FileChooserDriving {
    private let pid: pid_t
    private let application: AXUIElement
    private let identifier: String
    private let openControl: () throws -> Void
    private var panel: AXUIElement?

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
        _ = T17Activation.bringToFront(pid: pid)
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
        guard let panel else { return nil }
        return try nodes(panel).first { try attribute($0, kAXRoleAttribute) as? String == kAXSheetRole }
    }

    private func locationField() throws -> AXUIElement? {
        guard let sheet = try locationSheet() else { return nil }
        let descendants = try nodes(sheet)
        for role in [kAXTextFieldRole, kAXComboBoxRole] {
            if let field = try descendants.first(where: { try attribute($0, kAXRoleAttribute) as? String == role }) {
                return field
            }
        }
        return nil
    }

    private func openButton() throws -> AXUIElement? {
        guard let panel else { return nil }
        return try nodes(panel).first { try attribute($0, kAXIdentifierAttribute) as? String == "OKButton" }
    }

    private func key(_ code: CGKeyCode, flags: CGEventFlags = []) throws {
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false) else {
            throw T17FileChooser.failure("file chooser keyboard event creation failed")
        }
        down.flags = flags; up.flags = flags
        // Never send a private path or Return to whichever unrelated app steals focus.
        down.postToPid(pid); up.postToPid(pid)
    }

    private func action(_ element: AXUIElement, _ name: String) throws {
        let result = AXUIElementPerformAction(element, name as CFString)
        guard result == .success else {
            throw T17FileChooser.failure("file chooser \(name) failed; ax_error=\(result.rawValue)")
        }
    }

    private func attribute(_ element: AXUIElement, _ name: String) throws -> AnyObject? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(element, name as CFString, &value)
        if result == .noValue || result == .attributeUnsupported { return nil }
        guard result == .success else {
            throw T17FileChooser.failure("file chooser \(name) read failed; ax_error=\(result.rawValue)")
        }
        return value
    }

    private func nodes(_ root: AXUIElement) throws -> [AXUIElement] {
        var pending = [root]
        var index = 0
        while index < pending.count {
            guard pending.count <= 12_000 else { throw T17FileChooser.failure("file chooser AX tree exceeded limit") }
            let item = pending[index]
            index += 1
            pending.append(contentsOf: try attribute(item, kAXChildrenAttribute) as? [AXUIElement] ?? [])
        }
        return pending
    }
}
