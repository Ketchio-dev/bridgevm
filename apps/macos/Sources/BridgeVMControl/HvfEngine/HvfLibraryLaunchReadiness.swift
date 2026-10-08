import Darwin
import Foundation

extension HvfLibraryLaunchContext {
    var readinessIssues: [HvfWindowsReadinessIssue] {
        var issues: [HvfWindowsReadinessIssue] = []
        if VMRelocationJournal.isPending(config, rootURL: rootURL) {
            issues.append(.init(code: "relocation-pending", scope: .launch,
                summary: "VM relocation recovery is unresolved. Check both bundles and registration before starting."))
        }
        let transaction = HvfWindowsInstallFinalization.paths(slug: config.slug, libraryRoot: rootURL,
                                                              bundlePath: config.bundlePath).transaction
        var info = stat()
        // Any residual entry or failed lookup blocks writes. A missing journal
        // inside a partly removed directory does not authorize runtime launch.
        if lstat(transaction.path, &info) == 0 || errno != ENOENT {
            issues.append(.init(code: "install-cleanup-pending", scope: .launch,
                summary: "Windows installation transaction cleanup is unresolved. Reload the library; if it remains blocked, inspect the preserved transaction. Do not reinstall."))
        }
        return issues
    }
}
