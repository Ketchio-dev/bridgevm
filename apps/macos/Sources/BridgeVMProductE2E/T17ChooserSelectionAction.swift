enum T17ChooserSelectionAction {
    @discardableResult
    static func perform<Node>(budget: T17ChooserNativeBudget,
                              lookup: () throws -> Node?, enabled: (Node) throws -> Bool?,
                              press: (Node) throws -> Void) throws -> Bool {
        guard let button = try budget.read(.buttonLookup, lookup),
              try budget.read(.enabled, { try enabled(button) }) == true else { return false }
        try T17ChooserSelectionPress.perform(budget: budget) { try press(button) }
        return true
    }
}
