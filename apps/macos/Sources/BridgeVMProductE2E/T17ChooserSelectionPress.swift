enum T17ChooserSelectionPress {
    /// A press outcome can be uncertain; no outer polling loop may replay it.
    static func perform(budget: T17ChooserNativeBudget, press: () throws -> Void) throws {
        var entered = false
        do {
            try budget.read(.selectionPress) {
                entered = true
                try press()
            }
        } catch let blocker as T17Blocker where entered {
            throw T17Blocker(code: blocker.code, detail: blocker.detail + "; selection_press_replay_refused=true")
        }
    }
}
