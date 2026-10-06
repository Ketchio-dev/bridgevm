/// Relationships belong to one successful snapshot attempt, never to a later action.
struct T17FileChooserCapturedGraph<Node> {
    let nodes: [Node]
    private let root: Node
    private let edges: [UInt: [(node: Node, related: [Node])]]
    private let limit: Int
    private let hash: (Node) -> UInt
    private let same: (Node, Node) -> Bool

    static func read(root: Node, limit: Int = 12_000,
                     related: (Node) throws -> [Node], hash: @escaping (Node) -> UInt,
                     same: @escaping (Node, Node) -> Bool) throws -> Self {
        var edges: [UInt: [(node: Node, related: [Node])]] = [:]
        let nodes = try T17FileChooserGraph.walk(root: root, limit: limit, related: { node in
            let children = try related(node)
            edges[hash(node), default: []].append((node, children))
            return children
        }, hash: hash, same: same)
        return Self(nodes: nodes, root: root, edges: edges, limit: limit, hash: hash, same: same)
    }

    /// A node reachable without the selected owner is not exclusively owned by it.
    /// This also prevents a cycle back to the application from including other windows.
    func ownedNodes(by owner: Node) throws -> [Node] {
        if same(owner, root) { return nodes }
        let outside = try T17FileChooserGraph.walk(root: root, limit: limit, related: { node in
            try relationships(node).filter { !same($0, owner) }
        }, hash: hash, same: same)
        var excluded: [UInt: [Node]] = [:]
        for node in outside { excluded[hash(node), default: []].append(node) }
        return try T17FileChooserGraph.walk(root: owner, limit: limit, related: { node in
            try relationships(node).filter { candidate in
                !excluded[hash(candidate), default: []].contains { same($0, candidate) }
            }
        }, hash: hash, same: same)
    }

    private func relationships(_ node: Node) throws -> [Node] {
        guard let entry = edges[hash(node)]?.first(where: { same($0.node, node) }) else {
            throw T17FileChooser.failure("chooser snapshot relationship was absent")
        }
        return entry.related
    }
}
