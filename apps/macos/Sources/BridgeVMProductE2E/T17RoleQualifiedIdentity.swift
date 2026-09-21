enum T17RoleQualifiedIdentity {
    static func find<Node>(
        _ requested: String, role expectedRole: String, in nodes: [Node],
        identifier: (Node) throws -> String?, role: (Node) throws -> String?
    ) throws -> Node? {
        var matches: [Node] = []
        var readFailure: Error?
        for node in nodes {
            let observedIdentifier: String?
            do {
                observedIdentifier = try identifier(node)
            } catch {
                if readFailure == nil { readFailure = error }
                continue
            }
            guard observedIdentifier == requested else { continue }
            guard try role(node) == expectedRole else { continue }
            matches.append(node)
        }
        if matches.count > 1 {
            throw T17Blocker(code: "ui-element-missing",
                             detail: "ambiguous accessibility identity: \(requested);role=\(expectedRole);matches=\(matches.count)")
        }
        if let match = matches.first { return match }
        if let readFailure { throw readFailure }
        return nil
    }
}
