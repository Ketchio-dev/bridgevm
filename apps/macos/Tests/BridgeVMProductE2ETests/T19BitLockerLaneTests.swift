import Foundation
import XCTest
@testable import BridgeVMProductE2E

/// Only `--bitlocker-restart-check` with the import mode asks for the check. The private lane result then
/// gains exactly `bitlocker_restart_check` with a fixed schema; without the flag it is byte-identical.
final class T19BitLockerLaneTests: XCTestCase {
    private typealias Result = T19BitLockerGuestResultTests
    private typealias Records = T17FirstRunHostRecordsTests
    private static let bootKeys: Set<String> = ["boot", "ready", "status", "reason", "edition", "conversion", "encryption_percent",
                                                "protection", "lock", "protectors", "pcr_profile", "fve_code", "pcr_read",
                                                "pcr0_sha256", "pcr7_sha256"]

    private func result() -> String {
        URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("bridgevm-import-result-\(UUID().uuidString).json").path
    }

    private func refusal(_ arguments: [String]) -> T17Blocker? {
        do { _ = try ProductE2ECLI.parse(arguments); return nil } catch { return error as? T17Blocker }
    }

    func testTheFlagIsAcceptedOnceAndOnlyForTheImportMode() throws {
        let path = result()
        XCTAssertFalse(try ProductE2ECLI.parse(["--windows-import-product-e2e", "--request", "/tmp/r.json", "--result", path]).bitLockerRestartCheck)
        XCTAssertFalse(try ProductE2ECLI.parse(["--windows-product-e2e", "--request", "/tmp/r.json", "--result", path]).bitLockerRestartCheck)
        for order in [["--bitlocker-restart-check", "--windows-import-product-e2e"], ["--windows-import-product-e2e", "--bitlocker-restart-check"]] {
            let parsed = try ProductE2ECLI.parse(order + ["--request", "/tmp/r.json", "--result", path])
            XCTAssertTrue(parsed.bitLockerRestartCheck); XCTAssertEqual(parsed.mode, .installedDiskImport)
        }
        XCTAssertEqual(refusal(["--windows-product-e2e", "--bitlocker-restart-check", "--request", "/tmp/r.json", "--result", path]),
                       T17Blocker(code: "invalid-request", detail: "the BitLocker restart check is T19 only"))
        XCTAssertEqual(refusal(["--windows-import-product-e2e", "--bitlocker-restart-check", "--bitlocker-restart-check",
                                "--request", "/tmp/r.json", "--result", path]),
                       T17Blocker(code: "invalid-request", detail: "duplicate BitLocker restart check"))
        for value in ["tpm-only", "=1"] {
            XCTAssertEqual(refusal(["--windows-import-product-e2e", "--bitlocker-restart-check\(value)", "--request", "/tmp/r.json", "--result", path])?.detail,
                           "unknown command option")
        }
        XCTAssertEqual(refusal(["--bitlocker-restart-check", "--request", "/tmp/r.json", "--result", path])?.detail,
                       "exact mode, request and result are required")
    }

    /// The extraction that kept ProductE2ECLI.swift within its budget left every refusal unchanged.
    func testOptionRefusalsAreUnchanged() throws {
        let path = result()
        XCTAssertEqual(refusal(["--windows-import-product-e2e", "--request"])?.detail, "missing option value")
        XCTAssertEqual(refusal(["--windows-import-product-e2e", "--request", "/a", "--request", "/b", "--result", path])?.detail, "duplicate request")
        XCTAssertEqual(refusal(["--windows-import-product-e2e", "--request", "/a", "--result", path, "--result", path])?.detail, "duplicate result")
        XCTAssertEqual(refusal(["--windows-import-product-e2e", "--request", "/a", "--result", NSTemporaryDirectory()])?.detail,
                       "result output is not an absent file in a regular directory")
        // A missing result directory throws the file system's own error, as before the extraction.
        XCTAssertThrowsError(try ProductE2ECLI.parse(["--windows-import-product-e2e", "--request", "/a", "--result", "/nonexistent-\(UUID().uuidString)/r.json"]))
    }

    func testTheRunnerHoldsEvidenceOnlyWhenAsked() throws {
        let fixture = try A9ImportRequestFixture()
        let request = try A9ImportRequest.load(fixture.requestURL)
        XCTAssertNil(A9ImportProductRunner(request: request).bitlocker)
        let asked = try XCTUnwrap(A9ImportProductRunner(request: request, bitLockerRestartCheck: true).bitlocker)
        XCTAssertEqual(asked, T19BitLockerEvidence())
        XCTAssertEqual(asked.outcome, .notRun); XCTAssertEqual(asked.reason, .journeyIncomplete); XCTAssertEqual(asked.stop, .notStarted)
    }

    func testWithoutEvidenceTheLaneResultIsByteIdentical() throws {
        let lane = try t19Lane()
        let log = T17TPMEvidenceLog(), bundle = try self.bundle()
        let plain = ProductE2EPrivateLaneResult(lane: T17FirstRunHostRecords.attach(lane, bundlePath: bundle), tpm: log.evidence)
        let wrapped = ProductE2EPrivateLaneResult(lane: T17FirstRunHostRecords.attach(lane, bundlePath: bundle).adding(nil), tpm: log.evidence)
        XCTAssertEqual(try bytes(wrapped), try bytes(plain))
    }

    func testTheFieldHasAFixedSchemaWithExplicitNulls() throws {
        let lane = try t19Lane(), log = T17TPMEvidenceLog(), bundle = try self.bundle()
        let base = try object(ProductE2EPrivateLaneResult(lane: T17FirstRunHostRecords.attach(lane, bundlePath: bundle), tpm: log.evidence))
        var evidence = T19BitLockerEvidence(outcome: .unlocked, reason: nil, restartObserved: true, powerCycleAfterRestart: true, stop: .guestShutdown)
        evidence.enable = T19BitLockerBootRecord(ready: true, reason: nil, state: Result.enabled)
        var unlocked = Result.enabled; unlocked.conversion = 1; unlocked.protection = 1; unlocked.percent = 100
        unlocked.pcr7SHA256 = String(repeating: "7c", count: 32)
        evidence.unlock = T19BitLockerBootRecord(ready: true, reason: nil, state: unlocked)
        var all = try object(ProductE2EPrivateLaneResult(
            lane: T17FirstRunHostRecords.attach(lane, bundlePath: bundle).adding(evidence), tpm: log.evidence))
        let field = try XCTUnwrap(all.removeValue(forKey: "bitlocker_restart_check") as? [String: Any])
        XCTAssertEqual(all as NSDictionary, base as NSDictionary, "every other field is unchanged")
        XCTAssertEqual(Set(field.keys), ["schema_version", "outcome", "reason", "boots", "guest_restart_observed",
                                         "power_cycle_after_restart", "pcr0_equal", "pcr7_equal", "vm_stop"])
        XCTAssertEqual(field["schema_version"] as? String, "bridgevm.windows-product-e2e-bitlocker-restart-check.v1")
        XCTAssertEqual(field["outcome"] as? String, "unlocked"); XCTAssertEqual(field["reason"] as? String, "none")
        XCTAssertEqual(field["pcr0_equal"] as? Bool, true); XCTAssertEqual(field["pcr7_equal"] as? Bool, false)
        XCTAssertEqual(field["vm_stop"] as? String, "guest-shutdown")
        let boots = try XCTUnwrap(field["boots"] as? [[String: Any]])
        XCTAssertEqual(boots.map { Set($0.keys) }, [Self.bootKeys, Self.bootKeys])
        XCTAssertEqual(boots[1] as NSDictionary, [
            "boot": "unlock", "ready": true, "status": "available", "reason": "none", "edition": "pro", "conversion": "encrypted",
            "encryption_percent": 100, "protection": "on", "lock": "unlocked", "protectors": ["tpm"], "pcr_profile": [7, 11],
            "fve_code": NSNull(), "pcr_read": "ok", "pcr0_sha256": Result.pcr0, "pcr7_sha256": String(repeating: "7c", count: 32),
        ] as NSDictionary)
        XCTAssertEqual(boots[0]["conversion"] as? String, "encrypting")
    }

    func testUnobservedFieldsAreNullAndEveryCodeHasAName() throws {
        let field = try object(T19BitLockerEvidence())
        XCTAssertEqual(field as NSDictionary, [
            "schema_version": T19BitLockerEvidence.schemaVersion, "outcome": "not-run", "reason": "journey-incomplete",
            "boots": [Self.unobserved("enable"), Self.unobserved("unlock")], "guest_restart_observed": NSNull(),
            "power_cycle_after_restart": NSNull(), "pcr0_equal": NSNull(), "pcr7_equal": NSNull(), "vm_stop": "not-started",
        ] as NSDictionary)
        var refused = T19BitLockerEvidence(outcome: .refused, reason: .protectorsPresent, stop: .productStop)
        refused.enable = T19BitLockerBootRecord(ready: true, reason: .protectorsPresent, state: T19BitLockerGuestState(
            outcome: .protectorsPresent, edition: nil, conversion: 5, protection: 2, lock: 1, percent: 3,
            protectors: Array(0...10), pcrProfile: [], fveCode: 0, pcrRead: "helper-failed"))
        let boot = try XCTUnwrap((try object(refused))["boots"] as? [[String: Any]])[0]
        XCTAssertEqual(boot["status"] as? String, "unavailable"); XCTAssertEqual(boot["reason"] as? String, "protectors-present")
        XCTAssertEqual(boot["conversion"] as? String, "decryption-paused"); XCTAssertEqual(boot["protection"] as? String, "unknown")
        XCTAssertEqual(boot["lock"] as? String, "locked"); XCTAssertTrue(boot["edition"] is NSNull); XCTAssertTrue(boot["pcr0_sha256"] is NSNull)
        XCTAssertEqual(boot["protectors"] as? [String], ["other", "tpm", "external-key", "numerical-password", "tpm-pin", "tpm-startup-key",
                                                        "tpm-pin-startup-key", "public-key", "passphrase", "tpm-certificate", "cng"])
        XCTAssertEqual(boot["pcr_profile"] as? [Int], []); XCTAssertEqual(boot["fve_code"] as? Int, 0)
        XCTAssertEqual(T19BitLockerEvidence.conversions.count, 6); XCTAssertEqual(T19BitLockerEvidence.protections.count, 3)
    }

    private static func unobserved(_ boot: String) -> NSDictionary {
        var values: [String: Any] = Dictionary(uniqueKeysWithValues: bootKeys.map { ($0, NSNull()) })
        values["boot"] = boot; values["status"] = "unavailable"; values["reason"] = "not-reached"
        return values as NSDictionary
    }

    private func t19Lane() throws -> A9ImportLaneResult {
        let fixture = try A9ImportRequestFixture()
        return A9ImportEvidence(nonce: String(repeating: "a", count: 64)).result(
            request: try A9ImportRequest.load(fixture.requestURL), failureCode: "none", failureDetail: "",
            cleanupVerified: true, uiFrontendAutomated: true)
    }

    private func bundle() throws -> String {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-bitlocker-lane-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: bundle) }
        let evidence = bundle.appendingPathComponent("metadata/product-e2e", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        let tail = Records.records(Records.poll, Records.stream, Records.repeats, Records.timing) + Records.teardown
        try Data(Records.report(tail: tail).utf8).write(to: evidence.appendingPathComponent(T17FirstRunHostRecords.archive))
        return bundle.path
    }

    /// The bytes T17ResultWriter writes for `value`.
    private func bytes<Value: Encodable>(_ value: Value) throws -> Data {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-lane-\(UUID().uuidString).json")
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        try T17ResultWriter.write(value, to: url)
        return try Data(contentsOf: url)
    }

    private func object<Value: Encodable>(_ value: Value) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: bytes(value)) as? [String: Any])
    }
}
