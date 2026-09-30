import ApplicationServices

/// Failure-only detail for one failed AX attribute read: the failing node's role as a bounded
/// AX token, never titles, values or paths, and not re-read when AXRole itself failed. The role
/// precedes ax_error because retry classification matches that suffix.
enum T17AXReadFailure {
    static func blocker(_ node: AXUIElement, attribute: String, status: AXError) -> T17Blocker {
        T17Blocker(code: "ui-element-missing", detail: detail(attribute: attribute, status: status) {
            var role: CFTypeRef?
            return AXUIElementCopyAttributeValue(node, kAXRoleAttribute as CFString, &role) == .success ? role as? String : nil
        })
    }

    static func detail(attribute: String, status: AXError, role: () -> String?) -> String {
        let observed = attribute == kAXRoleAttribute ? nil : role()
        return "ax_tree_read_failed;attribute=\(attribute);role=\(token(observed));ax_error=\(status.rawValue)"
    }

    static func token(_ role: String?) -> String {
        guard let role else { return "unreadable" }
        let safe = role.utf8.count <= 48 && role.hasPrefix("AX")
            && role.utf8.allSatisfy { (65...90).contains($0) || (97...122).contains($0) }
        return safe ? role : "other"
    }
}
