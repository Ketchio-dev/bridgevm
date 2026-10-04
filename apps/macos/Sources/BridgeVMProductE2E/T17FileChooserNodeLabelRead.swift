enum T17FileChooserNodeLabelRead {
    static func naming<Node, Result>(_ node: Node, describe: (Node) -> String,
                                     read: () throws -> Result) throws -> Result {
        do { return try read() }
        catch let blocker as T17Blocker {
            // Deadline refusal already has fixed labels; do not start AX diagnostics for it.
            if T17ChooserNativeBudget.isDeadlineFailure(blocker) { throw blocker }
            throw T17Blocker(code: blocker.code, detail: "node=" + describe(node) + "; " + blocker.detail)
        }
    }
}
