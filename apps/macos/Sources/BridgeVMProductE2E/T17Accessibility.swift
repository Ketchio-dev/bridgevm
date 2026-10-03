import AppKit
import ApplicationServices
import Foundation

final class T17Accessibility: T17UIControlling {
    let pid: pid_t
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
        try perform(element(identifier, role: T17PressAction.targetRole, timeout: timeout), identifier, timeout)
    }

    /// SwiftUI exposes a disclosure group's label as an AXDisclosureTriangle, not an AXButton.
    func expand(_ identifier: String, timeout: TimeInterval = 10) throws {
        try perform(element(identifier, role: kAXDisclosureTriangleRole as String, timeout: timeout), identifier, timeout)
    }

    private func perform(_ target: AXUIElement, _ identifier: String, _ timeout: TimeInterval) throws {
        try T17PressAction.perform(identifier: identifier, timeout: timeout, enabled: { (try T17SupportedAttribute.read(target, kAXEnabledAttribute) as? NSNumber)?.boolValue },
            press: { AXUIElementPerformAction(target, kAXPressAction as CFString) }, activate: { T17Activation.bringToFront(pid: self.pid) },
            retry: { AXUIElementPerformAction(target, kAXPressAction as CFString) }, frontmost: { NSRunningApplication(processIdentifier: self.pid)?.isActive == true })
    }

    func setText(_ value: String, identifier: String, timeout: TimeInterval = 10) throws {
        let target = try element(identifier, role: kAXTextFieldRole as String, timeout: timeout)
        try T17TextEntry.commit(value, set: {
            AXUIElementSetAttributeValue(target, kAXValueAttribute as CFString, $0 as CFTypeRef) == .success
        }, confirm: { AXUIElementPerformAction(target, kAXConfirmAction as CFString) == .success }, read: {
            try T17SupportedAttribute.read(target, kAXValueAttribute) as? String
        })
    }

    func fill(_ value: String, identifier: String, timeout: TimeInterval = 10) throws {
        let target = try element(identifier, role: kAXTextFieldRole as String, timeout: timeout)
        try T17TextEntry.fill(value, focus: { AXUIElementSetAttributeValue(target, kAXFocusedAttribute as CFString, kCFBooleanTrue) == .success },
            set: { AXUIElementSetAttributeValue(target, kAXValueAttribute as CFString, $0 as CFTypeRef) == .success },
            read: { try T17SupportedAttribute.read(target, kAXValueAttribute) as? String })
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
        try T17ChooserAdmission.choose(path: path, timeout: timeout, lookup: {
            try self.element(identifier, role: T17ChooserOpenAction.targetRole, timeout: $0)
        }) { target, deadline, now in
            T17FileChooserAX(pid: self.pid, selection: try T17ChooserSelectionTarget.resolve(button: identifier)) { try T17ChooserOpenAction.perform(
                identifier: identifier, deadline: deadline,
                enabled: { (try T17SupportedAttribute.read(target, kAXEnabledAttribute) as? NSNumber)?.boolValue }, press: { AXUIElementPerformAction(target, kAXPressAction as CFString) }, now: now) }
        }
    }

    func textSnapshot() -> [String] {
        ((try? descendants(of: AXUIElementCreateApplication(pid), limit: 12_000)) ?? []).compactMap { item in
            for name in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
                if let value = attribute(item, name as CFString) as? String, !value.isEmpty { return value }
            }
            return nil
        }
    }

    func clickDisplaySurface(at spot: CGPoint, timeout: TimeInterval = 15) throws {
        let application = AXUIElementCreateApplication(pid)
        try T17DisplayClick.click(at: spot, timeout: timeout, read: {
            guard let surface = try? descendants(of: application, limit: 12_000).first(where: {
                attribute($0, kAXIdentifierAttribute as CFString) as? String == T17DisplayClick.surface
            }) else { return nil }
            let window = attribute(surface, kAXWindowAttribute as CFString)
            let focused = attribute(application, kAXFocusedWindowAttribute as CFString)
            let frame = point(surface).flatMap { origin in size(surface).map { CGRect(origin: origin, size: $0) } }
            return T17DisplayClick.Target(value: attribute(surface, kAXValueAttribute as CFString) as? String,
                                          focused: window.flatMap { w in focused.map { CFEqual(w, $0) } } ?? false, frame: frame)
        }, post: T17DisplayClick.post)
    }

    private func element(_ identifier: String, role expectedRole: String? = nil,
                         timeout: TimeInterval) throws -> AXUIElement {
        try T17IdentifierSearch.find(identifier, role: expectedRole, timeout: timeout,
            root: { AXUIElementCreateApplication(pid) }, nodes: { try descendants(of: $0, limit: 12_000) },
            attribute: { try T17SupportedAttribute.read($0, $1) as? String }, missing: {
                T17MissingIdentifierDiagnostic.capture(application: AXUIElementCreateApplication(pid), pid: pid,
                                                       identifier: identifier, timeout: timeout)
            })
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
