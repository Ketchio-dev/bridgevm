import ApplicationServices

enum T17FileChooserAXTree {
    static let relationships = [kAXChildrenAttribute, kAXWindowsAttribute]
    static func applicationNodes(pid: pid_t) throws -> [AXUIElement] {
        try T17RetryingSnapshot.read(attempts: 3, root: { AXUIElementCreateApplication(pid) }, retryable: {
                ($0 as? T17Blocker)?.detail.contains(" read failed; ax_error=\(AXError.invalidUIElement.rawValue)") == true
            }, snapshot: nodes)
    }
    static func attribute(_ element: AXUIElement, _ name: String) throws -> AnyObject? {
        var value: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(element, name as CFString, &value)
        if result == .noValue || result == .attributeUnsupported { return nil }
        guard result == .success else {
            throw T17FileChooser.failure("file chooser \(name) read failed; ax_error=\(result.rawValue)")
        }
        return value
    }

    static func nodes(_ root: AXUIElement) throws -> [AXUIElement] {
        try T17FileChooserGraph.walk(root: root, limit: 12_000, related: { node in
            try relationships.flatMap { try attribute(node, $0) as? [AXUIElement] ?? [] }
        }, hash: { CFHash($0) }, same: { CFEqual($0, $1) })
    }
}
