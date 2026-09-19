import ApplicationServices

enum T17ApplicationSnapshotFailure {
    static func isRetryable(_ error: Error) -> Bool {
        guard let blocker = error as? T17Blocker,
              blocker.code == "ui-element-missing",
              blocker.detail.hasPrefix("ax_tree_read_failed;") else { return false }
        return [AXError.invalidUIElement, AXError.failure].contains {
            blocker.detail.hasSuffix("ax_error=\($0.rawValue)")
        }
    }

    static func attributed(_ error: Error, identifier: String) -> Error {
        guard let blocker = error as? T17Blocker else { return error }
        return T17Blocker(code: blocker.code,
                          detail: "\(blocker.detail);stage=identifier-search;identifier=\(identifier)")
    }
}
