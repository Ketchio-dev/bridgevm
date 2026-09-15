@MainActor
struct HvfWindowsInstallLifecycle {
    var workAdmission: LibraryWorkAdmission?
    var onCompleted: (() -> Void)?
}

extension HvfWindowsInstallSession {
    var workAdmission: LibraryWorkAdmission? {
        get { lifecycle.workAdmission }
        set { lifecycle.workAdmission = newValue }
    }

    /// Called after the transaction has durably published the completed config.
    var onCompleted: (() -> Void)? {
        get { lifecycle.onCompleted }
        set { lifecycle.onCompleted = newValue }
    }
}
