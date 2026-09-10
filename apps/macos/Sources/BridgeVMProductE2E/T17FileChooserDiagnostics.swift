import AppKit
import ApplicationServices

extension T17FileChooserDriving {
    var failureContext: String { "" }
}

extension T17FileChooser {
    static func diagnosticSuffix(_ driver: T17FileChooserDriving) -> String {
        let context = String(driver.failureContext.prefix(900))
        return context.isEmpty ? "" : "; " + context
    }
}

/// Failure-only metadata: never expose window titles, text values or file paths.
enum T17FileChooserDiagnostics {
    static func label(_ value: Any?) -> String {
        guard let value = value as? String else { return "none" }
        let allowed = ["AXWindow", "AXSheet", "AXDialog", "AXTextField", "AXComboBox",
                       "AXOutline", "open-panel", "GoToWindow", "PathTextField", "OKButton", "CancelButton"]
        return allowed.contains(value) ? value : "other"
    }

    private static func attribute(_ node: AXUIElement, _ name: String) -> (AXError, CFTypeRef?) {
        var value: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(node, name as CFString, &value)
        return (status, value)
    }

    private static func element(_ value: CFTypeRef?) -> String {
        guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return "none" }
        let node: AXUIElement = unsafeBitCast(value, to: AXUIElement.self)
        return label(attribute(node, kAXRoleAttribute).1) + "/" + label(attribute(node, kAXIdentifierAttribute).1)
    }

    static func snapshot(pid: pid_t) -> String {
        let app = AXUIElementCreateApplication(pid)
        let front = attribute(app, kAXFrontmostAttribute)
        let windows = attribute(app, kAXWindowsAttribute)
        let focus = attribute(app, kAXFocusedUIElementAttribute)
        let inventory = windows.1 as? [AXUIElement]
        let windowLabels = (inventory ?? []).prefix(4).map { element($0) }.joined(separator: ",")
        let active = NSRunningApplication(processIdentifier: pid)?.isActive.description ?? "unknown"
        let axFront = (front.1 as? NSNumber)?.boolValue.description ?? "unknown"
        let frontPID = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? -1
        return "pid=\(pid),active=\(active),helper=\(NSRunningApplication.current.isActive),front=\(frontPID),"
            + "ax=\(front.0.rawValue)/\(axFront),windows=\(windows.0.rawValue)/\(inventory?.count ?? -1)[\(windowLabels)],"
            + "focus=\(focus.0.rawValue)[\(element(focus.1))]"
    }

    static func post(pid: pid_t, code: CGKeyCode, flags: CGEventFlags) throws -> String {
        guard let down = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: true),
              let up = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: false) else {
            throw T17FileChooser.failure("file chooser keyboard event creation failed")
        }
        let before = snapshot(pid: pid)
        down.flags = flags; up.flags = flags
        // Keep the existing PID-targeted transport; this change only records context.
        down.postToPid(pid); up.postToPid(pid)
        return "key=\(code)/\(flags.rawValue),before{\(before)},after{\(snapshot(pid: pid))}"
    }
}
