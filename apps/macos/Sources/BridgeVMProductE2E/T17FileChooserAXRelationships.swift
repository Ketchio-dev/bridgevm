import ApplicationServices

enum T17FileChooserAXRelationships {
    static func read(_ node: AXUIElement, _ name: String, io: T17FileChooserAXIO) throws -> [AXUIElement] {
        try decode(io.relationship(node, name))
    }
    static func decode(_ raw: AnyObject?) throws -> [AXUIElement] {
        guard let raw else { return [] }
        guard CFGetTypeID(raw) == CFArrayGetTypeID(), let values = raw as? [AnyObject],
              values.allSatisfy({ CFGetTypeID($0) == AXUIElementGetTypeID() }),
              let nodes = raw as? [AXUIElement] else {
            throw T17FileChooser.failure("chooser relationship payload was invalid")
        }
        return nodes
    }
}
