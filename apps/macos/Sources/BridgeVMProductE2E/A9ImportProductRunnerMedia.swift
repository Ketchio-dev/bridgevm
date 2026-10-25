import Foundation

/// The import lane's media authentication: the sealed source, the media the UI imported, the final
/// selected media and vTPM tree, and the source left unchanged.
extension A9ImportProductRunner {
    func authenticateSource(_ evidence: inout A9ImportEvidence) throws {
        try evidence.authenticate("source_disk_sha256", file: URL(fileURLWithPath: request.sourceDiskPath))
        try evidence.authenticate("source_vars_sha256", file: URL(fileURLWithPath: request.sourceVarsPath))
        try evidence.authenticateTree("source_vtpm_tree_sha256", root: URL(fileURLWithPath: request.sourceVtpmPath), fileManager: fileManager)
    }

    func authenticateImported(_ evidence: inout A9ImportEvidence) throws {
        try evidence.authenticate("imported_initial_disk_sha256", file: URL(fileURLWithPath: request.diskPath))
        try evidence.authenticate("imported_initial_vars_sha256", file: URL(fileURLWithPath: request.varsPath))
        try evidence.authenticateTree("imported_initial_vtpm_tree_sha256", root: URL(fileURLWithPath: request.vtpmStatePath), fileManager: fileManager)
    }

    func authenticateFinal(_ evidence: inout A9ImportEvidence) throws {
        try evidence.authenticateSelectedMedia(disk: request.diskPath, vars: request.varsPath)
        try evidence.authenticateTree("final_vtpm_tree_sha256", root: URL(fileURLWithPath: request.vtpmStatePath), fileManager: fileManager)
        try evidence.authenticate("guest_evidence_sha256", file: URL(fileURLWithPath: request.guestEvidencePath))
    }

    func requireMatchingInitialMedia(_ evidence: A9ImportEvidence) throws {
        guard evidence.hashes["source_disk_sha256"] == evidence.hashes["imported_initial_disk_sha256"],
              evidence.hashes["source_vars_sha256"] == evidence.hashes["imported_initial_vars_sha256"],
              evidence.hashes["source_vtpm_tree_sha256"] == evidence.hashes["imported_initial_vtpm_tree_sha256"] else {
            throw T17Blocker(code: "import-media-mismatch", detail: "UI-imported media differs from the sealed source")
        }
    }

    func requireUnchangedSource(_ evidence: A9ImportEvidence) throws {
        guard evidence.hashes["source_disk_sha256"] == (try T17Evidence.sha256(URL(fileURLWithPath: request.sourceDiskPath))),
              evidence.hashes["source_vars_sha256"] == (try T17Evidence.sha256(URL(fileURLWithPath: request.sourceVarsPath))),
              evidence.hashes["source_vtpm_tree_sha256"] == (try A9ImportTreeDigest.compute(URL(fileURLWithPath: request.sourceVtpmPath), fileManager: fileManager)) else {
            throw T17Blocker(code: "source-media-mutated", detail: "the installed-disk import changed its read-only source")
        }
    }
}
