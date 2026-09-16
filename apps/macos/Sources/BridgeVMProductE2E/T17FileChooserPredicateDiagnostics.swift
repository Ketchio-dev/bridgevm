/// Records only fixed labels and counters from the production predicate lookup.
final class T17FileChooserPredicateDiagnostics {
    enum Scope: String { case notReached = "not_reached", application, goToSheet = "go_to_sheet" }
    enum State: String {
        case notReached = "not_reached", walking, matching, complete
        case treeError = "tree_error", metadataError = "metadata_error", matchError = "match_error"
    }
    final class Lookup {
        var scope = Scope.notReached
        var attempted = false
        var state = State.notReached
        var nodes: Int?
        var visited: Int?
        var idMatches: Int?
        var roleMatches: Int?
        var errors: Int?
        var detail: String {
            func count(_ value: Int?) -> String { value?.description ?? "unknown" }
            return "scope=\(scope.rawValue),attempted=\(attempted),state=\(state.rawValue),"
                + "nodes=\(count(nodes)),visited=\(count(visited)),id_matches=\(count(idMatches)),"
                + "role_matches=\(count(roleMatches)),errors=\(count(errors))"
        }
    }

    private(set) var panelCached: Bool?
    private(set) var owner = Lookup()
    private(set) var field = Lookup()

    func beginOwner(panelCached: Bool) {
        self.panelCached = panelCached
        owner = Lookup()
        field = Lookup()
    }

    func find<Node>(in nodes: () throws -> [Node], id: String, roles: Set<String>,
                    metadata: (Node) throws -> (String?, String?),
                    same: (Node, Node) -> Bool) throws -> Node? {
        guard id == "GoToWindow" || id == "PathTextField" else {
            return try T17FileChooserIdentity.find(in: nodes(), id: id, roles: roles,
                                                  metadata: metadata, same: same)
        }
        let lookup = Lookup()
        lookup.attempted = true
        lookup.scope = id == "GoToWindow" ? .application : .goToSheet
        lookup.state = .walking
        if id == "GoToWindow" { owner = lookup } else { field = lookup }
        do {
            let candidates = try nodes()
            lookup.nodes = candidates.count
            lookup.visited = 0
            lookup.idMatches = 0
            lookup.roleMatches = 0
            lookup.errors = 0
            lookup.state = .matching
            let result = try T17FileChooserIdentity.find(in: candidates, id: id, roles: roles, metadata: {
                let value: (String?, String?)
                do { value = try metadata($0) }
                catch {
                    lookup.errors = 1
                    lookup.state = .metadataError
                    throw error
                }
                // Count complete metadata tuples only; a failure leaves a partial prefix.
                lookup.visited = (lookup.visited ?? 0) + 1
                if value.0 == id {
                    lookup.idMatches = (lookup.idMatches ?? 0) + 1
                    if let role = value.1, roles.contains(role) {
                        lookup.roleMatches = (lookup.roleMatches ?? 0) + 1
                    }
                }
                return value
            }, same: same)
            lookup.state = .complete
            return result
        } catch {
            if lookup.state == .walking { lookup.state = .treeError }
            else if lookup.state == .matching { lookup.state = .matchError }
            throw error
        }
    }

    var snapshot: String {
        "predicate{panel_cached=\(panelCached?.description ?? "unknown");"
            + "owner{\(owner.detail)};field{\(field.detail)}}"
    }

    /// Inputs are existing redacted diagnostics, never AX values or error descriptions.
    func context(activation: Bool?, key: String, timeout: String, tree: String) -> String {
        String(("\(snapshot); activation=\(activation?.description ?? "unknown"); \(key); "
            + "timeout{\(timeout)}; tree{\(tree)}").prefix(900))
    }
}
