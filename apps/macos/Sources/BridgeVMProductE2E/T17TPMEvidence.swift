import Foundation

/// The boots of the shared T17/T19 guest journey at which the agent is ready: first
/// READY, the boot after the snapshot is created, and the boot after it is restored.
enum T17TPMBoot: Int, CaseIterable { case first = 1, mutation, final }

/// Why a boot's TPM observation is unavailable. Host codes come first; the guest may
/// report only `guestReported`, plus the Get-Tpm failure `tpm-query-failed`.
enum T17TPMReason: String, Error, CaseIterable {
    case notReached = "not-reached", assetMissing = "asset-missing", launchFailed = "launch-failed"
    case resultMissing = "result-missing", resultUnsafe = "result-unsafe", resultMalformed = "result-malformed"
    case tpmQueryFailed = "tpm-query-failed", tbsContextFailed = "tbs-context-failed", tbsSubmitFailed = "tbs-submit-failed"
    case tpmErrorResponse = "tpm-error-response", pcrSelectionMismatch = "pcr-selection-mismatch"
    case pcrResponseMalformed = "pcr-response-malformed", helperFailed = "helper-failed"

    /// The failure codes bv-product-e2e-tpm.ps1 and its TBS helper write as `pcr_read`.
    static let guestReported: [T17TPMReason] = [
        .tbsContextFailed, .tbsSubmitFailed, .tpmErrorResponse, .pcrSelectionMismatch, .pcrResponseMalformed, .helperFailed,
    ]
}

/// One boot's observation: booleans and SHA-256 hashes of PCR values, never a value.
/// A nil reason means available; nil fields were not observed.
struct T17TPMBootRecord: Equatable {
    let boot: T17TPMBoot
    var reason: T17TPMReason?
    var tpmPresent: Bool?
    var tpmReady: Bool?
    var pcr0SHA256: String?
    var pcr7SHA256: String?
    /// Whether this boot's run log held a guest SYSTEM_RESET record before the read,
    /// so the PCRs were read after a helper respawn against the same swtpm.
    var guestRestartBeforeRead: Bool?

    static func unavailable(_ boot: T17TPMBoot, _ reason: T17TPMReason) -> Self { Self(boot: boot, reason: reason) }
}

struct T17TPMTimeouts: Equatable {
    var asset: TimeInterval = 60
    var launch: TimeInterval = 60
    var result: TimeInterval = 90
}

/// The lane's TPM evidence, shared by its runner and journey so that boots read
/// before a journey failure are still recorded. At most one record per boot.
final class T17TPMEvidenceLog {
    let timeouts: T17TPMTimeouts
    private var records: [T17TPMBoot: T17TPMBootRecord] = [:]

    init(timeouts: T17TPMTimeouts = T17TPMTimeouts()) { self.timeouts = timeouts }

    /// Each boot is read once; a second record for the same boot is ignored.
    func record(_ record: T17TPMBootRecord) {
        if records[record.boot] == nil { records[record.boot] = record }
    }

    var evidence: T17TPMEvidence { T17TPMEvidence(records: records) }
}

/// `tpm_evidence` in the private lane result: diagnostic only, so it never changes a
/// stage, a failure code or the public receipt.
struct T17TPMEvidence: Encodable, Equatable {
    static let schemaVersion = "bridgevm.windows-product-e2e-tpm-evidence.v1"
    let boots: [T17TPMBootRecord]

    init(records: [T17TPMBoot: T17TPMBootRecord]) {
        boots = T17TPMBoot.allCases.map { records[$0] ?? .unavailable($0, .notReached) }
    }

    /// Firmware measurements are deterministic, so a power-cycled vTPM gives every boot
    /// the first boot's PCR 0. Nil when either PCR 0 was not observed.
    func pcr0EqualsFirstBoot(_ record: T17TPMBootRecord) -> Bool? {
        guard let first = boots.first?.pcr0SHA256, let pcr0 = record.pcr0SHA256 else { return nil }
        return pcr0 == first
    }

    func encode(to encoder: Encoder) throws {
        var output = encoder.container(keyedBy: DynamicKey.self)
        try output.encode(Self.schemaVersion, forKey: .init("schema_version"))
        var list = output.nestedUnkeyedContainer(forKey: .init("boots"))
        for record in boots {
            var item = list.nestedContainer(keyedBy: DynamicKey.self)
            func put<Value: Encodable>(_ value: Value?, _ key: String) throws {
                if let value { try item.encode(value, forKey: .init(key)) } else { try item.encodeNil(forKey: .init(key)) }
            }
            try put(record.boot.rawValue, "boot")
            try put(record.reason == nil ? "available" : "unavailable", "status")
            try put(record.reason?.rawValue ?? "none", "reason")
            try put(record.tpmPresent, "tpm_present"); try put(record.tpmReady, "tpm_ready")
            try put(record.pcr0SHA256, "pcr0_sha256"); try put(record.pcr7SHA256, "pcr7_sha256")
            try put(pcr0EqualsFirstBoot(record), "pcr0_equals_first_boot")
            try put(record.guestRestartBeforeRead, "guest_restart_before_read")
        }
    }
}
