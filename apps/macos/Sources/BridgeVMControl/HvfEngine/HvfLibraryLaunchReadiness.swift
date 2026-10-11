import Foundation

extension HvfLibraryLaunchContext {
    var readinessIssues: [HvfWindowsReadinessIssue] {
        var issues: [HvfWindowsReadinessIssue] = []
        if VMRelocationJournal.isPending(config, rootURL: rootURL) {
            issues.append(.init(code: "relocation-pending", scope: .launch,
                summary: "VM relocation recovery is unresolved. Check both bundles and registration before starting."))
        }
        if HvfWindowsInstallTransactionResidue.isPresent(config, rootURL: rootURL) {
            issues.append(.init(code: "install-cleanup-pending", scope: .launch,
                summary: "Windows installation transaction cleanup is unresolved. Reload the library; if it remains blocked, inspect the preserved transaction. Do not reinstall."))
        }
        return issues
    }
}
