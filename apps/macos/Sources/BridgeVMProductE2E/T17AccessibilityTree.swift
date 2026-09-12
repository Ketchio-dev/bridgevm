import ApplicationServices

enum T17AccessibilityTree {
    static let relationships = [kAXChildrenAttribute, kAXWindowsAttribute]

    static func attribute(_ node: AXUIElement, _ name: String) throws -> AnyObject? {
        var value: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(node, name as CFString, &value)
        if status == .noValue || status == .attributeUnsupported { return nil }
        guard status == .success else {
            throw T17Blocker(code: "ui-element-missing",
                             detail: "ax_tree_read_failed;attribute=\(name);ax_error=\(status.rawValue)")
        }
        return value
    }

    static func nodes(_ root: AXUIElement, limit: Int) throws -> [AXUIElement] {
        do {
            return try T17FileChooserGraph.walk(root: root, limit: limit, related: { node in
                try relationships.flatMap { name -> [AXUIElement] in
                    guard let value = try attribute(node, name) else { return [] }
                    guard let nodes = value as? [AXUIElement] else {
                        throw T17Blocker(code: "ui-element-missing", detail: "ax_tree_invalid_relationship;attribute=\(name)")
                    }
                    return nodes
                }
            }, hash: { CFHash($0) }, same: { CFEqual($0, $1) })
        } catch let blocker as T17Blocker where blocker.code == "ui-element-missing" {
            throw blocker
        } catch {
            throw T17Blocker(code: "ui-element-missing", detail: "ax_tree_incomplete;node_limit=\(limit)")
        }
    }
}
