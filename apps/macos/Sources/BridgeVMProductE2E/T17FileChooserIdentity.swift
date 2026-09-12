enum T17FileChooserIdentity {
    static func find<Node>(
        in nodes: [Node], id: String, roles: Set<String>,
        metadata: (Node) throws -> (String?, String?), same: (Node, Node) -> Bool
    ) throws -> Node? {
        var match: Node?
        for node in nodes {
            let (identifier, role) = try metadata(node)
            guard identifier == id else { continue }
            guard let role, roles.contains(role) else {
                throw T17FileChooser.failure("identified chooser element has unexpected role")
            }
            if let previous = match, !same(previous, node) {
                throw T17FileChooser.failure("identified chooser element was ambiguous")
            }
            match = node
        }
        return match
    }
}
