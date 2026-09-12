enum T17CreationProbe {
    static let installIdentifier = "bridgevm.windows.install.view"
    static let errorIdentifier = "bridgevm.create.error"
    static let failureCodes: Set<String> = ["template-unavailable", "invalid-vm-name", "hvf-import-invalid",
                                            "vm-materialization-failed", "library-publication-failed"]

    static func find<Node>(_ requested: String, in nodes: [Node],
                           identifier: (Node) throws -> String?, value: (Node) throws -> String?) throws -> Node? {
        var match: Node?
        var failure: Node?
        for node in nodes {
            let id = try identifier(node)
            if id == requested, match == nil { match = node }
            if id == errorIdentifier, failure == nil { failure = node }
        }
        if requested == installIdentifier, let failure {
            let code = try value(failure) ?? ""
            let safe = failureCodes.contains(code) ? code : "creation-error-unclassified"
            throw T17Blocker(code: "vm-creation-failed", detail: "stage=create;reason=\(safe)")
        }
        return match
    }
}
