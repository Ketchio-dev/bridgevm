import Foundation

/// The guest result phases of the opt-in T19 BitLocker restart check
/// (docs/history/windows-hvf/bitlocker-restart-check-20261023.md).
enum T19BitLockerPhase: String, CaseIterable { case enable, status }

/// Why the check, or one of its boots, has no verdict. The guest reports only `guestReported`.
enum T19BitLockerReason: String, Error, CaseIterable {
    case journeyIncomplete = "journey-incomplete", logHoldFailed = "log-hold-failed", notReached = "not-reached"
    case bootNotReady = "boot-not-ready", assetMissing = "asset-missing", launchFailed = "launch-failed"
    case resultMissing = "result-missing", resultUnsafe = "result-unsafe", resultMalformed = "result-malformed"
    case restartNotObserved = "restart-not-observed", unlockBootNotReady = "unlock-boot-not-ready"
    case unlockStateUnexpected = "unlock-state-unexpected"
    case editionUnsupported = "edition-unsupported", tpmNotReady = "tpm-not-ready", bitlockerUnavailable = "bitlocker-unavailable"
    case volumeNotDecrypted = "volume-not-decrypted", protectorsPresent = "protectors-present"
    case protectorAddFailed = "protector-add-failed", protectorUnexpected = "protector-unexpected"
    case encryptFailed = "encrypt-failed", queryFailed = "query-failed", scriptFailed = "script-failed"

    /// The refusal codes bv-product-e2e-bitlocker.ps1 writes as `outcome`.
    static let guestReported: [T19BitLockerReason] = [
        .editionUnsupported, .tpmNotReady, .bitlockerUnavailable, .volumeNotDecrypted, .protectorsPresent,
        .protectorAddFailed, .protectorUnexpected, .encryptFailed, .queryFailed, .scriptFailed,
    ]
}

enum T19BitLockerOutcome: String { case unlocked, recoverySuspected = "recovery-suspected", refused, inconclusive, notRun = "not-run" }

/// How the check's VM ended: a guest shutdown to SYSTEM_OFF, the product's stop control, neither, or never started.
enum T19BitLockerStop: String { case guestShutdown = "guest-shutdown", productStop = "product-stop", unconfirmed, notStarted = "not-started" }

/// One guest result: fixed codes, small integers and PCR hashes. Nil was reported as unknown or absent.
struct T19BitLockerGuestState: Equatable {
    var outcome: T19BitLockerReason?
    var edition: String?
    var conversion: Int?, protection: Int?, lock: Int?, percent: Int?
    var protectors: [Int]?
    var pcrProfile: [Int]?
    var fveCode: Int?
    var pcrRead = "ok"
    var pcr0SHA256: String?, pcr7SHA256: String?
}

/// One boot of the check. A nil reason means its guest result was read and said `ok`.
struct T19BitLockerBootRecord: Equatable {
    var ready: Bool?
    var reason: T19BitLockerReason? = .notReached
    var state: T19BitLockerGuestState?
}

/// `bitlocker_restart_check` in the private T19 lane result: present only when the sealed manifest
/// asked for the check, and never read by a stage, a failure code or the public receipt.
struct T19BitLockerEvidence: Encodable, Equatable {
    static let key = "bitlocker_restart_check"
    static let schemaVersion = "bridgevm.windows-product-e2e-bitlocker-restart-check.v1"
    static let conversions = ["decrypted", "encrypted", "encrypting", "decrypting", "encryption-paused", "decryption-paused"]
    static let protections = ["off", "on", "unknown"], locks = ["unlocked", "locked"]
    static let protectorTypes = ["other", "tpm", "external-key", "numerical-password", "tpm-pin", "tpm-startup-key",
                                 "tpm-pin-startup-key", "public-key", "passphrase", "tpm-certificate", "cng"]
    var outcome = T19BitLockerOutcome.notRun
    var reason: T19BitLockerReason? = .journeyIncomplete
    var enable = T19BitLockerBootRecord(), unlock = T19BitLockerBootRecord()
    var restartObserved: Bool?
    var powerCycleAfterRestart: Bool?
    var stop = T19BitLockerStop.notStarted

    func equal(_ hash: KeyPath<T19BitLockerGuestState, String?>) -> Bool? {
        guard let first = enable.state?[keyPath: hash], let second = unlock.state?[keyPath: hash] else { return nil }
        return first == second
    }

    func encode(to encoder: Encoder) throws {
        var output = encoder.container(keyedBy: DynamicKey.self)
        try output.encode(Self.schemaVersion, forKey: .init("schema_version"))
        try output.encode(outcome.rawValue, forKey: .init("outcome"))
        try output.encode(reason?.rawValue ?? "none", forKey: .init("reason"))
        var boots = output.nestedUnkeyedContainer(forKey: .init("boots"))
        for (name, record) in [("enable", enable), ("unlock", unlock)] {
            var item = boots.nestedContainer(keyedBy: DynamicKey.self)
            func put<Value: Encodable>(_ value: Value?, _ key: String) throws {
                if let value { try item.encode(value, forKey: .init(key)) } else { try item.encodeNil(forKey: .init(key)) }
            }
            let state = record.state
            try put(name, "boot"); try put(record.ready, "ready")
            try put(record.reason == nil ? "available" : "unavailable", "status"); try put(record.reason?.rawValue ?? "none", "reason")
            try put(state?.edition, "edition"); try put(state?.conversion.map { Self.conversions[$0] }, "conversion")
            try put(state?.percent, "encryption_percent"); try put(state?.protection.map { Self.protections[$0] }, "protection")
            try put(state?.lock.map { Self.locks[$0] }, "lock"); try put(state?.protectors?.map { Self.protectorTypes[$0] }, "protectors")
            try put(state?.pcrProfile, "pcr_profile"); try put(state?.fveCode, "fve_code"); try put(state?.pcrRead, "pcr_read")
            try put(state?.pcr0SHA256, "pcr0_sha256"); try put(state?.pcr7SHA256, "pcr7_sha256")
        }
        for (key, value) in [("guest_restart_observed", restartObserved), ("power_cycle_after_restart", powerCycleAfterRestart),
                             ("pcr0_equal", equal(\.pcr0SHA256)), ("pcr7_equal", equal(\.pcr7SHA256))] {
            if let value { try output.encode(value, forKey: .init(key)) } else { try output.encodeNil(forKey: .init(key)) }
        }
        try output.encode(stop.rawValue, forKey: .init("vm_stop"))
    }
}
