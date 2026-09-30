import ApplicationServices

/// Failure-only detail for one failed AX attribute read. The failing node's role
/// is reduced to a bounded AX role token; titles, values and paths are never read.
/// The role precedes ax_error because retry classification matches that suffix.
enum T17AXReadFailure {
    static func blocker(_ node: AXUIElement, attribute: String, status: AXError) -> T17Blocker {
        var role: CFTypeRef?
        let read = attribute != kAXRoleAttribute
            && AXUIElementCopyAttributeValue(node, kAXRoleAttribute as CFString, &role) == .success
        return T17Blocker(code: "ui-element-missing",
                          detail: detail(attribute: attribute, role: read ? role as? String : nil, status: status))
    }

    static func detail(attribute: String, role: String?, status: AXError) -> String {
        "ax_tree_read_failed;attribute=\(attribute);role=\(token(role));ax_error=\(status.rawValue)"
    }

    static func token(_ role: String?) -> String {
        guard let role else { return "unreadable" }
        let safe = role.utf8.count <= 48 && role.hasPrefix("AX")
            && role.utf8.allSatisfy { (65...90).contains($0) || (97...122).contains($0) }
        return safe ? role : "other"
    }
}
