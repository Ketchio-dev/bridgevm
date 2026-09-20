enum T17CreationProbe {
    static let installIdentifier = "bridgevm.windows.install.view"
    static let errorIdentifier = "bridgevm.create.error"
    static let failureCodes: Set<String> = ["template-unavailable", "invalid-vm-name", "hvf-import-invalid",
                                            "vm-materialization-failed", "library-publication-failed"]

    static func find<Node>(_ requested: String, in nodes: [Node],
                           identifier: (Node) throws -> String?, value: (Node) throws -> String?) throws -> Node? {
        let projection = T17IdentifierProjection.collect(
            [requested, errorIdentifier], in: nodes, identifier: identifier)
        let match = projection.matches[requested]
        let failure = projection.matches[errorIdentifier]
        if requested == installIdentifier, let failure {
            let code = try value(failure) ?? ""
            let safe = failureCodes.contains(code) ? code : "creation-error-unclassified"
            throw T17Blocker(code: "vm-creation-failed", detail: "stage=create;reason=\(safe)")
        }
        if let match { return match }
        if let readFailure = projection.readFailure { throw readFailure }
        return nil
    }
}
