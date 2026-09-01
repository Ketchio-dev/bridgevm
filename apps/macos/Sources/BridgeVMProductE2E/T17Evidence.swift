import CryptoKit
import Foundation

struct T17Evidence {
    static let hashFields = [
        "installer_source_sha256", "final_disk_sha256", "final_vars_sha256",
        "secure_boot_receipt_sha256", "guest_evidence_sha256",
    ]

    private(set) var stages = Dictionary(uniqueKeysWithValues: T17Stage.allCases.map { ($0, false) })
    private(set) var hashes: [String: String]
    let nonce: String

    init(nonce: String) {
        self.nonce = nonce
        hashes = Dictionary(uniqueKeysWithValues: Self.hashFields.map {
            ($0, Self.sentinel(nonce: nonce, field: $0))
        })
    }

    mutating func prove(_ stage: T17Stage) throws {
        guard let index = T17Stage.allCases.firstIndex(of: stage),
              T17Stage.allCases[..<index].allSatisfy({ stages[$0] == true }) else {
            throw T17Blocker(code: "internal-error", detail: "stage evidence was recorded out of order")
        }
        stages[stage] = true
    }

    mutating func authenticate(_ field: String, file: URL) throws {
        guard Self.hashFields.contains(field) else {
            throw T17Blocker(code: "internal-error", detail: "unknown evidence hash field")
        }
        let values = try file.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        guard values.isRegularFile == true, values.isSymbolicLink != true else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "evidence file is missing or unsafe")
        }
        hashes[field] = try Self.sha256(file)
    }

    func result(request: T17Request, failureCode: String, cleanupVerified: Bool,
                installerSourcePath: String, uiFrontendAutomated: Bool) -> T17LaneResult {
        let complete = T17Stage.allCases.allSatisfy { stages[$0] == true }
        return T17LaneResult(
            jobID: request.jobID, commit: request.commit, campaignMode: request.campaignMode,
            lane: request.lane, nonce: request.nonce,
            uiFrontendAutomated: uiFrontendAutomated,
            failureCode: complete && cleanupVerified ? "none" : failureCode,
            cleanupVerified: cleanupVerified, installerSourcePath: installerSourcePath,
            stages: stages, hashes: hashes
        )
    }

    static func sha256(_ file: URL) throws -> String {
        guard let handle = FileHandle(forReadingAtPath: file.path) else {
            throw T17Blocker(code: "guest-evidence-missing", detail: "cannot read evidence file")
        }
        defer { try? handle.close() }
        var digest = SHA256()
        while true {
            let chunk = try handle.read(upToCount: 1024 * 1024) ?? Data()
            if chunk.isEmpty { break }
            digest.update(data: chunk)
        }
        return digest.finalize().map { String(format: "%02x", $0) }.joined()
    }

    private static func sentinel(nonce: String, field: String) -> String {
        let body = Data("bridgevm-t17-unproven-v1\n\(nonce)\n\(field)\n".utf8)
        return SHA256.hash(data: body).map { String(format: "%02x", $0) }.joined()
    }
}

enum T17ResultWriter {
    static func write(_ result: T17LaneResult, to output: URL) throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(result)
        try data.write(to: output, options: [.withoutOverwriting])
    }
}
