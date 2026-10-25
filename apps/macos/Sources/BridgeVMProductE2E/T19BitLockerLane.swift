import Foundation

/// A lane result with `bitlocker_restart_check` added to its own keyed container. Without
/// evidence (the check was not requested) it encodes exactly the lane it wraps.
struct T19BitLockerLane<Lane: Encodable>: Encodable {
    let lane: Lane
    let evidence: T19BitLockerEvidence?

    func encode(to encoder: Encoder) throws {
        try lane.encode(to: encoder)
        guard let evidence else { return }
        var output = encoder.container(keyedBy: DynamicKey.self)
        try output.encode(evidence, forKey: .init(T19BitLockerEvidence.key))
    }
}

extension T17HostRecordLane {
    func adding(_ evidence: T19BitLockerEvidence?) -> T19BitLockerLane<Self> { T19BitLockerLane(lane: self, evidence: evidence) }
}
