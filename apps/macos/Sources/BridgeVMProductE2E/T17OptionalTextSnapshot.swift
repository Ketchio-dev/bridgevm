enum T17OptionalTextSnapshot {
    static func read<Node>(
        pause: () -> Void, root: () -> Node, nodes: (Node) throws -> [Node],
        expected: Set<String>, identifier: (Node) throws -> String?,
        text: (Node) throws -> String
    ) throws -> [String: String] {
        try T17ApplicationSnapshot.read(attempts: 5, pause: pause, root: root, nodes: nodes) {
            try project(expected, in: $0, identifier: identifier, text: text)
        }
    }

    static func project<Node>(
        _ expected: Set<String>, in nodes: [Node],
        identifier: (Node) throws -> String?, text: (Node) throws -> String
    ) throws -> [String: String] {
        let projection = T17IdentifierProjection.collect(expected, in: nodes, identifier: identifier)
        if projection.matches.count < expected.count, let failure = projection.readFailure {
            throw failure
        }
        var values: [String: String] = [:]
        for (identifier, node) in projection.matches { values[identifier] = try text(node) }
        return values
    }

    static func attributed(_ error: Error, identifiers: Set<String>) -> Error {
        guard let blocker = error as? T17Blocker else { return error }
        let names = identifiers.sorted().joined(separator: ",")
        return T17Blocker(code: blocker.code,
                          detail: "\(blocker.detail);stage=identifier-search;identifiers=\(names)")
    }
}
