import Foundation
import XCTest
@testable import BridgeVMProductE2E

// BridgeVMProductE2EMain nests the first run's host records (T17FirstRunHostRecords)
// inside the TPM wrapper (ProductE2EPrivateLaneResult). The written lane result then
// holds every lane field unchanged plus both private keys, or only tpm_evidence when
// the host printed no records. product-e2e-lane-private-keys-test.py checks that both
// receipt writers admit such a result.
final class ProductE2EPrivateLaneCompositionTests: XCTestCase {
    private typealias Records = T17FirstRunHostRecordsTests

    func testNestedWrappersAddBothPrivateKeysToT17AndT19Lanes() throws {
        let nonce = String(repeating: "a", count: 64), evidence = T17Evidence(nonce: nonce)
        let t17 = T17LaneResult(
            jobID: "fixture", commit: String(repeating: "b", count: 40), campaignMode: "pilot", lane: 1, nonce: nonce,
            uiFrontendAutomated: true, failureCode: "none", failureDetail: "", cleanupVerified: true,
            installerSourcePath: "/private/tmp/source.raw", stages: evidence.stages, hashes: evidence.hashes)
        let fixture = try A9ImportRequestFixture()
        let t19 = A9ImportEvidence(nonce: nonce).result(
            request: try A9ImportRequest.load(fixture.requestURL), failureCode: "guest-evidence-missing",
            failureDetail: "detail", cleanupVerified: true, uiFrontendAutomated: true)
        let tail = Records.records(Records.poll, Records.stream, Records.repeats, Records.timing) + Records.teardown
        let printed = try bundle(Records.report(tail: tail)), silent = try bundle(Records.report(tail: Records.teardown))
        try check(t17, printed: printed, silent: silent)
        try check(t19, printed: printed, silent: silent)
    }

    private func check<Lane: Encodable>(_ lane: Lane, printed: String, silent: String) throws {
        let log = T17TPMEvidenceLog()
        let base = try written(lane), tpm = try written(log.evidence)
        let both = try written(ProductE2EPrivateLaneResult(
            lane: T17FirstRunHostRecords.attach(lane, bundlePath: printed), tpm: log.evidence))
        XCTAssertEqual(Set(both.keys), Set(base.keys).union([T17FirstRunHostRecords.key, "tpm_evidence"]))
        var rest = both
        let records = rest.removeValue(forKey: T17FirstRunHostRecords.key) as? [String: [String: NSNumber]]
        XCTAssertEqual(rest.removeValue(forKey: "tpm_evidence") as? NSDictionary, tpm as NSDictionary)
        XCTAssertEqual(rest as NSDictionary, base as NSDictionary, "every lane field is unchanged")
        XCTAssertEqual(records?.mapValues { $0.mapValues(\.uint64Value) }, Records.expected)
        let onlyTPM = try written(ProductE2EPrivateLaneResult(
            lane: T17FirstRunHostRecords.attach(lane, bundlePath: silent), tpm: log.evidence))
        XCTAssertEqual(Set(onlyTPM.keys), Set(base.keys).union(["tpm_evidence"]), "no records: only tpm_evidence")
    }

    /// The object T17ResultWriter writes for `value`.
    private func written<Value: Encodable>(_ value: Value) throws -> [String: Any] {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-lane-\(UUID().uuidString).json")
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        try T17ResultWriter.write(value, to: url)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    /// A lane bundle whose archived first run log holds `log`.
    private func bundle(_ log: String) throws -> String {
        let bundle = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-first-run-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: bundle) }
        let evidence = bundle.appendingPathComponent("metadata/product-e2e", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        try Data(log.utf8).write(to: evidence.appendingPathComponent(T17FirstRunHostRecords.archive))
        return bundle.path
    }
}
