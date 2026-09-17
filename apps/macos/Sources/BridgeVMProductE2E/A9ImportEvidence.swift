import CryptoKit
import Foundation

enum A9ImportStage: String, CaseIterable {
    case artifactPreflight = "artifact_preflight"
    case sourceAuthenticated = "source_authenticated"
    case uiImported = "ui_imported"
    case importedMediaAuthenticated = "imported_media_authenticated"
    case firstReady = "first_ready"
    case keyboardPointer = "keyboard_pointer"
    case clipboard
    case folderShare = "folder_share"
    case network, audio
    case firstShutdown = "first_shutdown"
    case snapshotRestore = "snapshot_restore"
    case secondReady = "second_ready"
    case secondShutdown = "second_shutdown"

    init(guest: T17GuestStage) {
        switch guest {
        case .keyboardPointer: self = .keyboardPointer
        case .clipboard: self = .clipboard
        case .folderShare: self = .folderShare
        case .network: self = .network
        case .audio: self = .audio
        case .firstShutdown: self = .firstShutdown
        case .snapshotRestore: self = .snapshotRestore
        case .secondReady: self = .secondReady
        case .secondShutdown: self = .secondShutdown
        }
    }
}

struct A9ImportEvidence {
    static let hashFields = [
        "source_disk_sha256", "source_vars_sha256", "source_vtpm_tree_sha256",
        "imported_initial_disk_sha256", "imported_initial_vars_sha256",
        "imported_initial_vtpm_tree_sha256", "final_disk_sha256", "final_vars_sha256",
        "final_vtpm_tree_sha256", "guest_evidence_sha256",
    ]
    private(set) var stages = Dictionary(uniqueKeysWithValues: A9ImportStage.allCases.map { ($0, false) })
    private(set) var hashes: [String: String]
    let nonce: String

    init(nonce: String) {
        self.nonce = nonce
        hashes = Dictionary(uniqueKeysWithValues: Self.hashFields.map { ($0, Self.sentinel(nonce, $0)) })
    }

    mutating func prove(_ stage: A9ImportStage) throws {
        guard let index = A9ImportStage.allCases.firstIndex(of: stage),
              A9ImportStage.allCases[..<index].allSatisfy({ stages[$0] == true }) else {
            throw T17Blocker(code: "internal-error", detail: "import stage evidence was recorded out of order")
        }
        stages[stage] = true
    }

    mutating func authenticate(_ field: String, file: URL) throws {
        guard Self.hashFields.contains(field) else {
            throw T17Blocker(code: "internal-error", detail: "unknown import evidence hash field")
        }
        hashes[field] = try T17Evidence.sha256(file)
    }

    mutating func authenticateTree(_ field: String, root: URL, fileManager: FileManager = .default) throws {
        guard Self.hashFields.contains(field) else {
            throw T17Blocker(code: "internal-error", detail: "unknown import tree evidence hash field")
        }
        hashes[field] = try A9ImportTreeDigest.compute(root, fileManager: fileManager)
    }

    func result(request: A9ImportRequest, failureCode: String, failureDetail: String,
                cleanupVerified: Bool, uiFrontendAutomated: Bool) -> A9ImportLaneResult {
        let complete = A9ImportStage.allCases.allSatisfy { stages[$0] == true }
        return A9ImportLaneResult(request: request, uiFrontendAutomated: uiFrontendAutomated,
            failureCode: complete && cleanupVerified ? "none" : failureCode,
            failureDetail: complete && cleanupVerified ? "" : failureDetail,
            cleanupVerified: cleanupVerified, stages: stages, hashes: hashes)
    }

    private static func sentinel(_ nonce: String, _ field: String) -> String {
        SHA256.hash(data: Data("bridgevm-a9-import-unproven-v1\n\(nonce)\n\(field)\n".utf8))
            .map { String(format: "%02x", $0) }.joined()
    }
}

struct A9ImportLaneResult: Encodable {
    let schemaVersion = "bridgevm.windows-hvf-import-product-e2e-lane.v1"
    let request: A9ImportRequest
    let uiFrontendAutomated: Bool
    let failureCode: String
    let failureDetail: String
    let cleanupVerified: Bool
    let stages: [A9ImportStage: Bool]
    let hashes: [String: String]

    func encode(to encoder: Encoder) throws {
        var out = encoder.container(keyedBy: DynamicKey.self)
        try out.encode(schemaVersion, forKey: .init("schema_version"))
        try out.encode(request.jobID, forKey: .init("job_id")); try out.encode(request.commit, forKey: .init("commit"))
        try out.encode(request.campaignMode, forKey: .init("campaign_mode")); try out.encode(request.lane, forKey: .init("lane"))
        try out.encode(request.nonce, forKey: .init("nonce")); try out.encode(false, forKey: .init("three_d_injection"))
        try out.encode(uiFrontendAutomated, forKey: .init("ui_frontend_automated"))
        try out.encode(failureCode, forKey: .init("failure_code")); try out.encode(failureDetail, forKey: .init("failure_detail"))
        try out.encode(cleanupVerified, forKey: .init("cleanup_verified"))
        for stage in A9ImportStage.allCases { try out.encode(stages[stage] == true, forKey: .init(stage.rawValue)) }
        for field in A9ImportEvidence.hashFields { try out.encode(hashes[field]!, forKey: .init(field)) }
    }
}
