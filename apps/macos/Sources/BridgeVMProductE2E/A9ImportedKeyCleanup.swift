import Foundation

/// Accounts for the imported key of every candidate slug in the lane (see A9KeyCleanupCandidates): each must be
/// forgotten or shown absent by the product (A9KeyForgetCall). forget-import opens the package against a state that
/// must match it, and a booted lane has rewritten its own, so the sealed source state the package was made from is
/// used; the tier re-hashes that source after cleanup.
enum A9ImportedKeyCleanup {
    static func run(request: A9ImportRequest, fileManager: FileManager, timeout: TimeInterval = 120) -> Bool {
        guard let slugs = A9KeyCleanupCandidates.slugs(request: request, fileManager: fileManager) else { return false }
        return slugs.map { forget($0, request: request, timeout: timeout) != .failed }.allSatisfy { $0 }
    }

    private static func forget(_ slug: String, request: A9ImportRequest, timeout: TimeInterval) -> A9KeyForgetCall.Outcome {
        A9KeyForgetCall.run(executable: request.appExecutablePath, arguments: [
            "--vtpm-lifecycle", "forget-import",
            "--stable-vm-id", slug,
            "--state-dir", request.sourceVtpmPath,
            "--package", request.sourceVtpmPackagePath,
            "--recovery-code-file", request.sourceVtpmCodePath,
        ], slug: slug, timeout: timeout)
    }
}
