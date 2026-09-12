import ApplicationServices

enum T17FileChooserAXTree {
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
