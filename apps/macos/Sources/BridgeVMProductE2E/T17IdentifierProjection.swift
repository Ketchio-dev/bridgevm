enum T17IdentifierProjection {
    static func collect<Node>(
        _ expected: Set<String>, in nodes: [Node],
        identifier: (Node) throws -> String?
    ) -> (matches: [String: Node], readFailure: Error?) {
        var matches: [String: Node] = [:]
        var readFailure: Error?
        for node in nodes {
            do {
                guard let value = try identifier(node),
                      expected.contains(value), matches[value] == nil else { continue }
                matches[value] = node
            } catch {
                if readFailure == nil { readFailure = error }
            }
        }
        return (matches, readFailure)
    }
}
