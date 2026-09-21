import AppKit
import ApplicationServices
import Foundation

protocol T17UIControlling {
    func press(_ identifier: String, timeout: TimeInterval) throws
    func setText(_ value: String, identifier: String, timeout: TimeInterval) throws
    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval) throws
    func choose(path: String, from identifier: String, timeout: TimeInterval) throws
    func waitFor(_ identifier: String, timeout: TimeInterval) throws
    func text(_ identifier: String, timeout: TimeInterval) throws -> String
    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String]
    func clickSecondaryWindow(timeout: TimeInterval) throws
    func textSnapshot() -> [String]
}

final class T17Accessibility: T17UIControlling {
    private let pid: pid_t

    init(pid: pid_t) throws {
        guard AXIsProcessTrusted() else {
            throw T17Blocker(code: "accessibility-untrusted", detail: T17TrustDiagnostic.detail())
        }
        self.pid = pid
    }

    func waitFor(_ identifier: String, timeout: TimeInterval) throws {
        _ = try element(identifier, timeout: timeout)
    }

    func text(_ identifier: String, timeout: TimeInterval = 10) throws -> String {
        let target = try element(identifier, timeout: timeout)
        return try textValue(target)
    }

    func optionalTexts(_ identifiers: Set<String>) throws -> [String: String] {
        do {
            return try T17OptionalTextSnapshot.read(pause: {
                Thread.sleep(forTimeInterval: 0.5)
            }, root: { AXUIElementCreateApplication(self.pid) }, nodes: {
                try self.descendants(of: $0, limit: 12_000)
            }, expected: identifiers, identifier: {
                try T17SupportedAttribute.read($0, kAXIdentifierAttribute) as? String
            }, text: self.textValue)
        } catch { throw T17OptionalTextSnapshot.attributed(error, identifiers: identifiers) }
    }

    private func textValue(_ target: AXUIElement) throws -> String {
        for name in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
            if let value = try T17SupportedAttribute.read(target, name) as? String { return value }
        }
        throw T17Blocker(code: "ui-element-missing", detail: "identified UI element has no text")
    }

    func press(_ identifier: String, timeout: TimeInterval = 10) throws {
        let target = try element(identifier, timeout: timeout)
        let first = AXUIElementPerformAction(target, kAXPressAction as CFString); if first == .success { return }
        let activated = T17Activation.bringToFront(pid: pid); let retry = activated ? AXUIElementPerformAction(target, kAXPressAction as CFString) : nil
        guard retry == .success else { throw T17Blocker(code: "ui-element-missing", detail: "AXPress failed: \(identifier); first_ax_error=\(first.rawValue); retry_ax_error=\(retry.map { String($0.rawValue) } ?? "not-attempted"); activation_succeeded=\(activated); frontmost=\(NSRunningApplication(processIdentifier: pid)?.isActive == true)") }
    }

    func setText(_ value: String, identifier: String, timeout: TimeInterval = 10) throws {
        let target = try element(identifier, role: kAXTextFieldRole as String, timeout: timeout)
        try T17TextEntry.commit(value, set: {
            AXUIElementSetAttributeValue(target, kAXValueAttribute as CFString, $0 as CFTypeRef) == .success
        }, confirm: { AXUIElementPerformAction(target, kAXConfirmAction as CFString) == .success }, read: {
            try T17SupportedAttribute.read(target, kAXValueAttribute) as? String
        })
    }

    func setToggle(_ enabled: Bool, identifier: String, timeout: TimeInterval = 10) throws {
        let target = try element(identifier, timeout: timeout)
        if let current = attribute(target, kAXValueAttribute as CFString) as? NSNumber,
           current.boolValue == enabled { return }
        guard AXUIElementPerformAction(target, kAXPressAction as CFString) == .success else {
            throw T17Blocker(code: "ui-element-missing", detail: "identified toggle cannot be changed")
        }
        guard let current = attribute(target, kAXValueAttribute as CFString) as? NSNumber,
              current.boolValue == enabled else {
            throw T17Blocker(code: "ui-element-missing", detail: "identified toggle did not reach the requested value")
        }
    }

    func choose(path: String, from identifier: String, timeout: TimeInterval = 15) throws {
        let driver = T17FileChooserAX(pid: pid, identifier: identifier) {
            try self.press(identifier, timeout: timeout)
        }
        try T17FileChooser.choose(path: path, timeout: timeout, driver: driver)
    }

    func textSnapshot() -> [String] {
        ((try? descendants(of: AXUIElementCreateApplication(pid), limit: 12_000)) ?? []).compactMap { item in
            for name in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
                if let value = attribute(item, name as CFString) as? String, !value.isEmpty { return value }
            }
            return nil
        }
    }

    func clickSecondaryWindow(timeout: TimeInterval = 10) throws {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            let application = AXUIElementCreateApplication(pid)
            let windows = attribute(application, kAXWindowsAttribute as CFString) as? [AXUIElement] ?? []
            for window in windows {
                let title = attribute(window, kAXTitleAttribute as CFString) as? String ?? ""
                guard title != "BridgeVM Control", let point = point(window), let size = size(window),
                      size.width > 320, size.height > 240 else { continue }
                let center = CGPoint(x: point.x + size.width / 2, y: point.y + size.height / 2)
                guard let move = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved,
                                         mouseCursorPosition: center, mouseButton: .left),
                      let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown,
                                         mouseCursorPosition: center, mouseButton: .left),
                      let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp,
                                       mouseCursorPosition: center, mouseButton: .left) else { continue }
                move.post(tap: .cghidEventTap); down.post(tap: .cghidEventTap); up.post(tap: .cghidEventTap)
                return
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        throw T17Blocker(code: "ui-element-missing", detail: "guest display window was not available for pointer input")
    }

    private func element(_ identifier: String, role expectedRole: String? = nil,
                         timeout: TimeInterval) throws -> AXUIElement {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            let match = try snapshotElement(identifier, role: expectedRole)
            if let match { return match }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        let application = AXUIElementCreateApplication(pid)
        throw T17Blocker(code: "ui-element-missing", detail: "required accessibility identifier was not found: \(identifier); windows=\((attribute(application, kAXWindowsAttribute as CFString) as? [AXUIElement]).map { String($0.count) } ?? "unanswered") timeout_s=\(timeout)")
    }

    private func snapshotElement(_ identifier: String, role expectedRole: String?) throws -> AXUIElement? {
        do {
            return try T17ApplicationSnapshot.read(
                root: { AXUIElementCreateApplication(self.pid) },
                nodes: { try self.descendants(of: $0, limit: 12_000) },
                project: { nodes in
                    if let expectedRole {
                        return try T17RoleQualifiedIdentity.find(identifier, role: expectedRole, in: nodes,
                            identifier: { try T17SupportedAttribute.read($0, kAXIdentifierAttribute) as? String },
                            role: { try T17SupportedAttribute.read($0, kAXRoleAttribute) as? String })
                    }
                    return try T17CreationProbe.find(identifier, in: nodes, identifier: {
                        try T17SupportedAttribute.read($0, kAXIdentifierAttribute) as? String
                    }, value: { try T17SupportedAttribute.read($0, kAXValueAttribute) as? String })
                })
        } catch { throw T17ApplicationSnapshotFailure.attributed(error, identifier: identifier) }
    }

    private func descendants(of root: AXUIElement, limit: Int) throws -> [AXUIElement] {
        try T17AccessibilityTree.nodes(root, limit: limit)
    }

    private func role(of element: AXUIElement) -> String? {
        attribute(element, kAXRoleAttribute as CFString) as? String
    }

    private func attribute(_ element: AXUIElement, _ name: CFString) -> AnyObject? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name, &value) == .success else { return nil }
        return value
    }

    private func point(_ element: AXUIElement) -> CGPoint? {
        guard let value = attribute(element, kAXPositionAttribute as CFString),
              CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
        let axValue: AXValue = unsafeBitCast(value, to: AXValue.self)
        var point = CGPoint.zero
        return AXValueGetValue(axValue, .cgPoint, &point) ? point : nil
    }

    private func size(_ element: AXUIElement) -> CGSize? {
        guard let value = attribute(element, kAXSizeAttribute as CFString),
              CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
        let axValue: AXValue = unsafeBitCast(value, to: AXValue.self)
        var size = CGSize.zero
        return AXValueGetValue(axValue, .cgSize, &size) ? size : nil
    }

}
enum T17TextEntry {
    static func commit(_ value: String, set: (String) -> Bool, confirm: () -> Bool, read: () throws -> String?) throws {
        guard set(value) else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element does not accept text") }
        guard confirm() else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element did not commit text") }
        guard try read() == value else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element did not retain exact text") }
    }
}
