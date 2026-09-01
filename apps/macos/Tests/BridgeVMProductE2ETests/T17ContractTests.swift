import Foundation
import XCTest
@testable import BridgeVMProductE2E

final class T17ContractTests: XCTestCase {
    private var roots: [URL] = []

    override func tearDown() {
        for root in roots { try? FileManager.default.removeItem(at: root) }
        roots = []
        super.tearDown()
    }

    func testStrictRequestAcceptsFixedProductPaths() throws {
        let fixture = try makeFixture()
        let request = try T17Request.load(fixture.request)
        XCTAssertEqual(request.vmSlug, fixture.slug)
        XCTAssertEqual(request.diskPath, fixture.bundle.appendingPathComponent("disks/hvf-target.raw").path)
    }

    func testRequestRejectsUnknownAndDuplicateFields() throws {
        let fixture = try makeFixture()
        var object = try XCTUnwrap(JSONSerialization.jsonObject(
            with: Data(contentsOf: fixture.request)) as? [String: Any])
        object["unreviewed"] = true
        let unknown = fixture.root.appendingPathComponent("unknown.json")
        try JSONSerialization.data(withJSONObject: object).write(to: unknown)
        XCTAssertThrowsError(try T17Request.load(unknown))

        let valid = try String(contentsOf: fixture.request)
        let duplicate = fixture.root.appendingPathComponent("duplicate.json")
        try valid.replacingOccurrences(of: "\"job_id\" :", with: "\"job_id\" : \"duplicate\", \"job_id\" :")
            .write(to: duplicate, atomically: false, encoding: .utf8)
        XCTAssertThrowsError(try T17Request.load(duplicate))
    }

    func testRequestRejectsSecureReceiptOutsideFixedBundlePath() throws {
        let fixture = try makeFixture()
        var object = try XCTUnwrap(JSONSerialization.jsonObject(
            with: Data(contentsOf: fixture.request)) as? [String: Any])
        object["secure_boot_receipt_path"] = fixture.root.appendingPathComponent("forged.json").path
        let forged = fixture.root.appendingPathComponent("forged-request.json")
        try JSONSerialization.data(withJSONObject: object).write(to: forged)
        XCTAssertThrowsError(try T17Request.load(forged))
    }

    func testEvidenceCannotSkipStagesAndUnprovenHashesAreNonceBound() throws {
        var first = T17Evidence(nonce: "1".repeated(64))
        XCTAssertThrowsError(try first.prove(.vmCreated))
        try first.prove(.artifactPreflight)
        let second = T17Evidence(nonce: "2".repeated(64))
        XCTAssertNotEqual(first.hashes["final_disk_sha256"], second.hashes["final_disk_sha256"])
    }

    func testCLIRequiresExactModeAndAbsentResult() throws {
        let fixture = try makeFixture()
        let result = fixture.root.appendingPathComponent("result.json")
        XCTAssertNoThrow(try T17CLI.parse([
            "--windows-product-e2e", "--request", fixture.request.path, "--result", result.path,
        ]))
        XCTAssertThrowsError(try T17CLI.parse(["--request", fixture.request.path, "--result", result.path]))
        try Data().write(to: result)
        XCTAssertThrowsError(try T17CLI.parse([
            "--windows-product-e2e", "--request", fixture.request.path, "--result", result.path,
        ]))
    }

    func testLaneResultRecordsWhetherAXFrontendStarted() throws {
        let fixture = try makeFixture()
        let request = try T17Request.load(fixture.request)
        var evidence = T17Evidence(nonce: request.nonce)
        try evidence.prove(.artifactPreflight)
        let result = evidence.result(
            request: request, failureCode: "accessibility-untrusted", cleanupVerified: true,
            installerSourcePath: "absent", uiFrontendAutomated: false)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(result)) as? [String: Any])
        XCTAssertEqual(object["ui_frontend_automated"] as? Bool, false)
        XCTAssertEqual(object["failure_code"] as? String, "accessibility-untrusted")
        XCTAssertEqual(Set(object.keys).count, 31)
    }

    func testRunLogProofBindsByteOffsetsLinesNonceAndAudioCounters() throws {
        let fixture = try makeFixture()
        let log = fixture.root.appendingPathComponent("run.log")
        let body = "prefix\nBVAGENT READY host=BRIDGEVM t=1\nhda CoreAudio stats: frames_rendered=48000 drops=0 callback_errors=0\nstop: PSCI SYSTEM_OFF\n"
        try Data(body.utf8).write(to: log)
        let proof = try T17RunLogProof.capture(
            log, nonce: String(repeating: "a", count: 64),
            readyTag: "first-ready", shutdownTag: "first-shutdown")
        XCTAssertEqual(proof.readyOffset, 7)
        XCTAssertGreaterThan(proof.shutdownOffset, proof.readyOffset)
        XCTAssertTrue(T17RunLogProof.audioPassed(log))
        let other = try T17RunLogProof.capture(
            log, nonce: String(repeating: "c", count: 64),
            readyTag: "first-ready", shutdownTag: "first-shutdown")
        XCTAssertNotEqual(proof.readyLineHash, other.readyLineHash)
    }

    func testPrivateUnattendUsesFreshCredentialsAndOwnerOnlyMode() throws {
        let fixture = try makeFixture()
        let first = fixture.root.appendingPathComponent("first.xml")
        let second = fixture.root.appendingPathComponent("second.xml")
        let nonce = String(repeating: "a", count: 64)
        try T17PrivateUnattend.write(to: first, nonce: nonce, fileManager: .default)
        try T17PrivateUnattend.write(to: second, nonce: nonce, fileManager: .default)
        let firstData = try Data(contentsOf: first), secondData = try Data(contentsOf: second)
        XCTAssertNotEqual(firstData, secondData)
        XCTAssertFalse(String(decoding: firstData, as: UTF8.self).contains(">bridge<"))
        XCTAssertTrue(String(decoding: firstData, as: UTF8.self).contains("BVT17aaaaaaaaaaaa"))
        XCTAssertFalse(String(decoding: firstData, as: UTF8.self)
            .replacingOccurrences(of: "\r\n", with: "").contains("\n"))
        let attributes = try FileManager.default.attributesOfItem(atPath: first.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
    }

    func testSecureBootReceiptMustMatchPackagedPolicy() throws {
        let fixture = try makeFixture()
        let hash = String(repeating: "d", count: 64)
        let policy: [String: Any] = [
            "schemaVersion": 1, "policy": "fixture-policy",
            "source": ["tag": "v1", "commit": "abc", "assetSha256": hash],
            "firmware": ["fileName": "firmware.fd", "sha256": hash, "edk2Commit": "def"],
            "variables": [["name": "PK", "vendorGuid": "guid", "attributes": 39, "sha256": hash]],
        ]
        var receipt: [String: Any] = [
            "schemaVersion": 1, "policy": "fixture-policy", "sourceTag": "v1",
            "sourceCommit": "abc", "sourceAssetSha256": hash,
            "firmwareFileName": "firmware.fd", "firmwareSha256": hash,
            "firmwareEdk2Commit": "def", "provisionedAt": "2026-09-01T12:00:00.123Z",
            "variables": [["name": "PK", "vendorGuid": "guid", "attributes": 39, "payloadSha256": hash]],
        ]
        let policyURL = fixture.root.appendingPathComponent("secure-policy.json")
        let receiptURL = fixture.root.appendingPathComponent("secure-receipt.json")
        try JSONSerialization.data(withJSONObject: policy).write(to: policyURL)
        try JSONSerialization.data(withJSONObject: receipt).write(to: receiptURL)
        XCTAssertNoThrow(try T17SecureBootReceipt.verify(receipt: receiptURL, policy: policyURL))
        receipt["firmwareSha256"] = String(repeating: "e", count: 64)
        try JSONSerialization.data(withJSONObject: receipt).write(to: receiptURL)
        XCTAssertThrowsError(try T17SecureBootReceipt.verify(receipt: receiptURL, policy: policyURL))
    }

    private func makeFixture() throws -> (root: URL, request: URL, bundle: URL, slug: String) {
        let root = URL(fileURLWithPath: "/tmp/bridgevm-e2e-swift-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        roots.append(root)
        let app = root.appendingPathComponent("BridgeVMControl.app", isDirectory: true)
        let executable = app.appendingPathComponent("Contents/MacOS/BridgeVMControl")
        let runner = app.appendingPathComponent("Contents/Resources/target/release/hvf-runner")
        let firmware = app.appendingPathComponent("Contents/Resources/firmware.fd")
        let policy = app.appendingPathComponent("Contents/Resources/policy.json")
        let seed = app.appendingPathComponent("Contents/Resources/seed.gz")
        for file in [executable, runner, firmware, policy, seed] {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            FileManager.default.createFile(atPath: file.path, contents: Data("fixture".utf8))
        }
        let iso = root.appendingPathComponent("windows.iso"); FileManager.default.createFile(atPath: iso.path, contents: Data("iso".utf8))
        let payload = root.appendingPathComponent("payload", isDirectory: true); try FileManager.default.createDirectory(at: payload, withIntermediateDirectories: false)
        FileManager.default.createFile(atPath: payload.appendingPathComponent("driver.sys").path, contents: Data("driver".utf8))
        let manifest = root.appendingPathComponent("payload.tsv"); FileManager.default.createFile(atPath: manifest.path, contents: Data("manifest".utf8))
        let nonce = String(repeating: "a", count: 64), slug = "bridgevm-t17-lane-1-aaaaaaaaaaaa"
        let library = root.appendingPathComponent("library", isDirectory: true)
        let bundle = library.appendingPathComponent(slug).appendingPathComponent("bundle.vmbridge")
        let object: [String: Any] = [
            "schema_version": "bridgevm.windows-hvf-3d-off-product-e2e-request.v2", "job_id": "fixture",
            "commit": String(repeating: "b", count: 40), "campaign_mode": "pilot", "lane": 1,
            "nonce": nonce, "vm_name": "BridgeVM T17 Lane 1 aaaaaaaaaaaa", "vm_slug": slug,
            "three_d_injection": false, "app_bundle_path": app.path, "app_executable_path": executable.path,
            "runner_path": runner.path, "firmware_path": firmware.path, "secure_boot_policy_path": policy.path,
            "iso_path": iso.path, "bundled_vars_seed_path": seed.path, "guest_payload_path": payload.path,
            "guest_payload_manifest_path": manifest.path, "lane_root": root.path, "library_root_path": library.path,
            "share_path": root.appendingPathComponent("share").path,
            "disk_path": bundle.appendingPathComponent("disks/hvf-target.raw").path,
            "vars_path": bundle.appendingPathComponent("metadata/hvf-vars.fd").path,
            "vtpm_state_path": bundle.appendingPathComponent("metadata/vtpm").path,
            "snapshot_path": bundle.appendingPathComponent("metadata/snapshots/latest.snapshot").path,
            "secure_boot_receipt_path": bundle.appendingPathComponent("metadata/secure-boot-provisioning.json").path,
            "guest_evidence_path": bundle.appendingPathComponent("metadata/product-e2e-guest-evidence.json").path,
        ]
        let request = root.appendingPathComponent("request.json")
        try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys]).write(to: request)
        return (root, request, bundle, slug)
    }
}

private extension String {
    func repeated(_ count: Int) -> String { String(repeating: self, count: count) }
}
