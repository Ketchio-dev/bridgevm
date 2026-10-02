import Foundation

extension T17Evidence {
    /// Final media are the pair the product boots next. After a restore the original paths keep
    /// their old bytes, and the host authenticator hashes the same selection (Studio T17 r80).
    mutating func authenticateSelectedMedia(disk: String, vars: String,
        digest: (String, String) throws -> T17SelectedMedia.Digest = { try T17SelectedMedia.digest(disk: $0, vars: $1) }) throws {
        let selected = try digest(disk, vars)
        hashes["final_disk_sha256"] = selected.diskSHA256
        hashes["final_vars_sha256"] = selected.varsSHA256
    }
}
