import Foundation

enum T17FileChooserSelectionLookup {
    static func read<Node>(budget: T17ChooserNativeBudget, root: () -> Node,
                           related: (Node) throws -> [Node], hash: @escaping (Node) -> UInt,
                           same: @escaping (Node, Node) -> Bool,
                           role: (Node) throws -> String?, identifier: (Node) throws -> String?,
                           pause: () -> Void = { Thread.sleep(forTimeInterval: 0.2) }) throws -> Node? {
        var graph: T17FileChooserCapturedGraph<Node>?
        return try budget.snapshot(root: root, nodes: { owner in
            graph = nil
            let fresh = try T17FileChooserCapturedGraph.read(root: owner, related: related, hash: hash, same: same)
            graph = fresh
            return fresh.nodes
        }, project: { candidates in
            guard let panel = try T17RoleFirstIdentity.find(in: candidates, id: "open-panel",
                roles: ["AXWindow", "AXSheet", "AXDialog"], role: role, identifier: identifier, same: same) else { return nil }
            guard let graph else { throw T17FileChooser.failure("chooser snapshot was absent") }
            return try T17RoleFirstIdentity.find(in: graph.ownedNodes(by: panel), id: "OKButton",
                roles: ["AXButton"], role: role, identifier: identifier, same: same)
        }, pause: pause)
    }
}
