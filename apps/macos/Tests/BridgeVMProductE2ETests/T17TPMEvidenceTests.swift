import XCTest
@testable import BridgeVMProductE2E

/// The private lane result records three boots with hashes and booleans only, compares
/// PCR 0 against the first boot, and adds nothing but tpm_evidence to either lane result.
final class T17TPMEvidenceTests: XCTestCase {
    private let pcr0 = String(repeating: "0a", count: 32), other = String(repeating: "0b", count: 32)
    private let pcr7 = String(repeating: "7c", count: 32)
    private let keys: Set<String> = ["boot", "status", "reason", "tpm_present", "tpm_ready", "pcr0_sha256",
                                     "pcr7_sha256", "pcr0_equals_first_boot", "guest_restart_before_read"]

    private func available(_ boot: T17TPMBoot, pcr0: String, restart: Bool? = false) -> T17TPMBootRecord {
        T17TPMBootRecord(boot: boot, reason: nil, tpmPresent: true, tpmReady: true, pcr0SHA256: pcr0,
                         pcr7SHA256: pcr7, guestRestartBeforeRead: restart)
    }

    private func encoded(_ evidence: T17TPMEvidence) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(evidence)) as? [String: Any])
    }

    private func boots(_ evidence: T17TPMEvidence) throws -> [[String: Any]] {
        try XCTUnwrap(encoded(evidence)["boots"] as? [[String: Any]])
    }

    func testUnreadBootsAreNotReachedAndEachBootKeepsItsFirstRecord() throws {
        let log = T17TPMEvidenceLog()
        XCTAssertEqual(log.evidence.boots, T17TPMBoot.allCases.map { T17TPMBootRecord.unavailable($0, .notReached) })
        log.record(available(.mutation, pcr0: pcr0))
        log.record(.unavailable(.mutation, .resultMissing))
        XCTAssertEqual(log.evidence.boots, [.unavailable(.first, .notReached), available(.mutation, pcr0: pcr0),
                                            .unavailable(.final, .notReached)])
        let encoded = try boots(log.evidence)
        XCTAssertEqual(encoded.map { $0["boot"] as? Int }, [1, 2, 3])
        XCTAssertEqual(encoded.map { $0["status"] as? String }, ["unavailable", "available", "unavailable"])
        XCTAssertEqual(encoded.map { $0["reason"] as? String }, ["not-reached", "none", "not-reached"])
    }

    func testPCR0IsComparedWithTheFirstBootOnly() throws {
        let log = T17TPMEvidenceLog()
        log.record(available(.first, pcr0: pcr0, restart: true))
        log.record(available(.mutation, pcr0: pcr0))
        log.record(available(.final, pcr0: other))
        XCTAssertEqual(try boots(log.evidence).map { $0["pcr0_equals_first_boot"] as? Bool }, [true, true, false])
        XCTAssertEqual(try boots(log.evidence).map { $0["guest_restart_before_read"] as? Bool }, [true, false, false])

        let unreadFirst = T17TPMEvidenceLog()
        unreadFirst.record(.unavailable(.first, .launchFailed))
        unreadFirst.record(available(.mutation, pcr0: pcr0))
        unreadFirst.record(available(.final, pcr0: pcr0))
        XCTAssertTrue(try boots(unreadFirst.evidence).allSatisfy { $0["pcr0_equals_first_boot"] is NSNull })

        let unreadLater = T17TPMEvidenceLog()
        unreadLater.record(available(.first, pcr0: pcr0))
        unreadLater.record(T17TPMBootRecord(boot: .mutation, reason: .tbsContextFailed, tpmPresent: false, tpmReady: false))
        let records = try boots(unreadLater.evidence)
        XCTAssertEqual(records[0]["pcr0_equals_first_boot"] as? Bool, true)
        XCTAssertTrue(records[1]["pcr0_equals_first_boot"] is NSNull)
        XCTAssertEqual(records[1]["tpm_present"] as? Bool, false)
        XCTAssertTrue(records[1]["pcr0_sha256"] is NSNull)
    }

    func testEncodedEvidenceHasExactKeysAndOnlyCodesHashesBooleansAndBootNumbers() throws {
        let log = T17TPMEvidenceLog()
        log.record(available(.first, pcr0: pcr0, restart: true))
        log.record(T17TPMBootRecord(boot: .mutation, reason: .tpmQueryFailed, pcr0SHA256: other, pcr7SHA256: pcr7,
                                    guestRestartBeforeRead: nil))
        let object = try encoded(log.evidence)
        XCTAssertEqual(Set(object.keys), ["schema_version", "boots"])
        XCTAssertEqual(object["schema_version"] as? String, "bridgevm.windows-product-e2e-tpm-evidence.v1")
        let records = try boots(log.evidence)
        XCTAssertEqual(records.count, 3)
        let codes = Set(T17TPMReason.allCases.map(\.rawValue) + ["none", "available", "unavailable"])
        for record in records {
            XCTAssertEqual(Set(record.keys), keys)
            for (key, value) in record where !(value is NSNull) {
                switch key {
                case "boot": XCTAssertTrue((1...3).contains(value as? Int ?? 0))
                case "status", "reason": XCTAssertTrue(codes.contains(value as? String ?? ""), key)
                case "pcr0_sha256", "pcr7_sha256":
                    XCTAssertNotNil((value as? String)?.range(of: "^[0-9a-f]{64}$", options: .regularExpression), key)
                default: XCTAssertTrue(value is Bool, key)
                }
            }
        }
        XCTAssertEqual(records[1]["status"] as? String, "unavailable")
        XCTAssertEqual(records[1]["reason"] as? String, "tpm-query-failed")
        XCTAssertEqual(records[1]["pcr0_equals_first_boot"] as? Bool, false)
        XCTAssertTrue(records[1]["guest_restart_before_read"] is NSNull)
        XCTAssertLessThan(try JSONEncoder().encode(log.evidence).count, 4096)
    }

    func testPrivateLaneResultAddsOnlyTPMEvidenceToTheT17Lane() throws {
        let owner = T17ContractTests(); defer { owner.tearDown() }
        let request = try T17Request.load(try owner.makeFixture().request)
        var evidence = T17Evidence(nonce: request.nonce)
        try evidence.prove(.artifactPreflight)
        let lane = evidence.result(request: request, failureCode: "guest-evidence-missing", failureDetail: "detail",
                                   cleanupVerified: true, installerSourcePath: "absent", uiFrontendAutomated: true)
        try assertOnlyTPMEvidenceAdded(lane, expectedKeys: 33)
    }

    func testPrivateLaneResultAddsOnlyTPMEvidenceToTheImportLane() throws {
        let fixture = try A9ImportRequestFixture()
        let request = try A9ImportRequest.load(fixture.requestURL)
        var evidence = A9ImportEvidence(nonce: request.nonce)
        try evidence.prove(.artifactPreflight)
        let lane = evidence.result(request: request, failureCode: "guest-evidence-missing", failureDetail: "detail",
                                   cleanupVerified: true, uiFrontendAutomated: true)
        try assertOnlyTPMEvidenceAdded(lane, expectedKeys: nil)
    }

    private func assertOnlyTPMEvidenceAdded<Lane: Encodable>(_ lane: Lane, expectedKeys: Int?) throws {
        let log = T17TPMEvidenceLog()
        log.record(available(.first, pcr0: pcr0))
        let plain = try XCTUnwrap(JSONSerialization.jsonObject(with: JSONEncoder().encode(lane)) as? NSDictionary)
        let wrapped = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(ProductE2EPrivateLaneResult(lane: lane, tpm: log.evidence))) as? NSDictionary)
        XCTAssertEqual(Set(wrapped.allKeys.compactMap { $0 as? String }),
                       Set(plain.allKeys.compactMap { $0 as? String }).union(["tpm_evidence"]))
        if let expectedKeys { XCTAssertEqual(wrapped.count, expectedKeys) }
        let withoutEvidence = NSMutableDictionary(dictionary: wrapped)
        withoutEvidence.removeObject(forKey: "tpm_evidence")
        XCTAssertEqual(withoutEvidence, NSMutableDictionary(dictionary: plain))
        XCTAssertEqual(wrapped["tpm_evidence"] as? NSDictionary,
                       try JSONSerialization.jsonObject(with: JSONEncoder().encode(log.evidence)) as? NSDictionary)
    }
}
