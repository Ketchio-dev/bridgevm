import Foundation

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
