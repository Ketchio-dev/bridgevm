enum LibraryDeletionOutcome: Equatable {
    case deleted
    case running
    case failed
}

enum LibraryDeletionAction {
    static func perform(isRunning: () -> Bool, delete: () -> Bool) -> LibraryDeletionOutcome {
        guard !isRunning() else { return .running }
        return delete() ? .deleted : .failed
    }
}
