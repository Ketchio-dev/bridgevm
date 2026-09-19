import ApplicationServices

enum T17SupportedAttribute {
    static func read(_ node: AXUIElement, _ name: String) throws -> AnyObject? {
        var rawNames: CFArray?
        let status = AXUIElementCopyAttributeNames(node, &rawNames)
        guard status == .success else {
            throw T17Blocker(code: "ui-element-missing",
                             detail: "ax_tree_read_failed;attribute=AXAttributeNames;ax_error=\(status.rawValue)")
        }
        guard let names = rawNames as? [String] else {
            throw T17Blocker(code: "ui-element-missing", detail: "ax_tree_invalid_attribute_names")
        }
        return try read(name, advertised: { names }) {
            try T17AccessibilityTree.attribute(node, name)
        }
    }

    static func read<Value>(
        _ name: String, advertised: () throws -> [String], value: () throws -> Value?
    ) rethrows -> Value? {
        guard try advertised().contains(name) else { return nil }
        return try value()
    }
}
