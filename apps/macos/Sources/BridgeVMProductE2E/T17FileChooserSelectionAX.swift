import ApplicationServices

enum T17FileChooserSelectionAX {
    static func read(pid: pid_t, io: T17FileChooserAXIO) throws -> AXUIElement? {
        // Each acceptance poll starts its own graph and retry sequence.
        try T17FileChooserSelectionLookup.read(budget: io.budget,
            root: { AXUIElementCreateApplication(pid) }, related: { node in
                let application = AXUIElementCreateApplication(pid)
                return try T17FileChooserNodeLabel.naming(node, describe: T17FileChooserNodeLabel.chain) {
                    try T17ApplicationWalkScope.relationships(of: node, root: application).flatMap {
                        try T17FileChooserAXRelationships.read(node, $0, io: io)
                    }
                }
            }, hash: { CFHash($0) }, same: { CFEqual($0, $1) },
            role: { try io.attribute($0, kAXRoleAttribute) as? String },
            identifier: { try io.attribute($0, kAXIdentifierAttribute) as? String })
    }
}
