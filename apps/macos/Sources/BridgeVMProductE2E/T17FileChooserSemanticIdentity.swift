enum T17FileChooserSemanticIdentity {
    /// Prefer Apple's stable identifier when it is present. Some macOS builds
    /// omit the Go To sheet identifiers, so a unique role inside the already
    /// scoped owner is the only permitted fallback.
    static func find<Node>(
        in nodes: [Node], id: String, roles: Set<String>,
        metadata: (Node) throws -> (String?, String?), same: (Node, Node) -> Bool
    ) throws -> Node? {
        if let identified = try T17FileChooserIdentity.find(
            in: nodes, id: id, roles: roles, metadata: metadata, same: same
        ) { return identified }

        var match: Node?
        for node in nodes {
            let (_, role) = try metadata(node)
            guard let role, roles.contains(role) else { continue }
            if let previous = match, !same(previous, node) {
                throw T17FileChooser.failure("semantic chooser element was ambiguous")
            }
            match = node
        }
        return match
    }
}
