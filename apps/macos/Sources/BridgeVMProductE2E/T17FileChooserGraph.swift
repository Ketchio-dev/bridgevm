enum T17FileChooserGraph {
    /// Hashes select a bucket; only identity equality suppresses a node.
    /// Window and sheet edges may alias children or lead back to the parent.
    static func walk<Node>(
        root: Node, limit: Int,
        related: (Node) throws -> [Node], hash: (Node) -> UInt,
        same: (Node, Node) -> Bool
    ) throws -> [Node] {
        guard limit > 0 else { throw T17FileChooser.failure("invalid AX graph limit") }
        var pending = [root]
        var seen = [hash(root): [root]]
        var index = 0
        while index < pending.count {
            let item = pending[index]
            index += 1
            for candidate in try related(item) {
                let key = hash(candidate)
                if seen[key, default: []].contains(where: { same($0, candidate) }) { continue }
                guard pending.count < limit else {
                    throw T17FileChooser.failure("file chooser AX tree exceeded limit")
                }
                seen[key, default: []].append(candidate)
                pending.append(candidate)
            }
        }
        return pending
    }
}
