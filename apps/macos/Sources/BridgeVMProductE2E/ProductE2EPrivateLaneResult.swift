import Foundation

/// The private lane result: every field of the lane result unchanged, plus
/// `tpm_evidence`. The receipt writers neither judge nor publish that key
/// (scripts/live-gates/product_e2e_private_lane.py), so it cannot change a lane.
struct ProductE2EPrivateLaneResult<Lane: Encodable>: Encodable {
    let lane: Lane
    let tpm: T17TPMEvidence

    func encode(to encoder: Encoder) throws {
        try lane.encode(to: encoder)
        var output = encoder.container(keyedBy: DynamicKey.self)
        try output.encode(tpm, forKey: .init("tpm_evidence"))
    }
}
