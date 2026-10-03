enum T17ChooserSelectionAction {
    static func perform<Node>(budget: T17ChooserNativeBudget,
                              lookup: () throws -> Node?, enabled: (Node) throws -> Bool?,
                              press: (Node) throws -> Void) throws {
        guard let button = try budget.read(.buttonLookup, lookup),
              try budget.read(.enabled, { try enabled(button) }) == true else {
            throw T17FileChooser.failure("Open button was not enabled at confirmation")
        }
        try budget.check(.selectionPress)
        try press(button)
    }
}
