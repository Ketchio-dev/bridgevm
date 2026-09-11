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
    func clickSecondaryWindow(timeout: TimeInterval) throws
    func textSnapshot() -> [String]
}

final class T17Accessibility: T17UIControlling {
    private let application: AXUIElement
    private let pid: pid_t

    init(pid: pid_t) throws {
        guard AXIsProcessTrusted() else {
            throw T17Blocker(code: "accessibility-untrusted", detail: T17TrustDiagnostic.detail())
        }
        self.pid = pid
        application = AXUIElementCreateApplication(pid)
    }

    func waitFor(_ identifier: String, timeout: TimeInterval) throws {
        _ = try element(identifier, timeout: timeout)
    }

    func text(_ identifier: String, timeout: TimeInterval = 10) throws -> String {
        let target = try element(identifier, timeout: timeout)
        for name in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
            if let value = attribute(target, name as CFString) as? String { return value }
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
        let identified = try element(identifier, timeout: timeout)
        let target = role(of: identified) == (kAXTextFieldRole as String)
            ? identified : (firstDescendant(of: identified, role: kAXTextFieldRole as String) ?? identified)
        guard AXUIElementSetAttributeValue(target, kAXValueAttribute as CFString, value as CFTypeRef) == .success else {
            throw T17Blocker(code: "ui-element-missing", detail: "identified UI element does not accept text")
        }
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
        descendants(of: application, limit: 12_000).compactMap { item in
            for name in [kAXValueAttribute, kAXTitleAttribute, kAXDescriptionAttribute] {
                if let value = attribute(item, name as CFString) as? String, !value.isEmpty { return value }
            }
            return nil
        }
    }

    func clickSecondaryWindow(timeout: TimeInterval = 10) throws {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
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

    private func element(_ identifier: String, timeout: TimeInterval) throws -> AXUIElement {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if let match = descendants(of: application, limit: 12_000).first(where: {
                attribute($0, kAXIdentifierAttribute as CFString) as? String == identifier
            }) { return match }
            RunLoop.current.run(until: Date().addingTimeInterval(0.1))
        } while Date() < deadline
        throw T17Blocker(code: "ui-element-missing", detail: "required accessibility identifier was not found: \(identifier); windows=\((attribute(application, kAXWindowsAttribute as CFString) as? [AXUIElement]).map { String($0.count) } ?? "unanswered") timeout_s=\(timeout)")
    }

    private func firstDescendant(of root: AXUIElement, role expected: String) -> AXUIElement? {
        descendants(of: root, limit: 128).first { role(of: $0) == expected }
    }

    private func descendants(of root: AXUIElement, limit: Int) -> [AXUIElement] {
        var output: [AXUIElement] = []
        var pending = [root]
        while !pending.isEmpty && output.count < limit {
            let item = pending.removeFirst()
            output.append(item)
            if let children = attribute(item, kAXChildrenAttribute as CFString) as? [AXUIElement] {
                pending.append(contentsOf: children)
            }
        }
        return output
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
