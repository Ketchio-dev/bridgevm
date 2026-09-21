enum T17RoleFirstIdentity {
    static func find<Node>(
        in nodes: [Node], id: String, roles: Set<String>,
        role: (Node) throws -> String?, identifier: (Node) throws -> String?,
        same: (Node, Node) -> Bool
    ) throws -> Node? {
        var match: Node?
        for node in nodes {
            guard let nodeRole = try role(node), roles.contains(nodeRole) else { continue }
            guard try identifier(node) == id else { continue }
            if let previous = match, !same(previous, node) {
                throw T17FileChooser.failure("identified chooser element was ambiguous")
            }
            match = node
        }
        return match
    }
}
