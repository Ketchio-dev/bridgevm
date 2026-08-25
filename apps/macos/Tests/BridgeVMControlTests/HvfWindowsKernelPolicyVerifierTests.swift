import CryptoKit
import XCTest
@testable import BridgeVMControl

final class HvfWindowsKernelPolicyVerifierTests: XCTestCase {
    private let issuedAt = "2026-08-25T00:00:00Z"
    private let expiresAt = "2026-08-26T00:00:00Z"

    func testSnapshotRefusesUnsafeDestinationPaths() throws { try assertUnsafeSnapshotPathsRefused() }

    func testValidSignatureAuthorizesPreflightAndAtomicSnapshot() throws {
        let fixture = try makeFixture()
        assertSuccess(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: fixture.package, now: fixture.now,
            trustAnchors: [fixture.anchor]))
        XCTAssertNil(HvfWindowsDriverPreflight.inspect(
            packageDirectory: fixture.package.path, now: fixture.now,
            trustAnchors: [fixture.anchor]).blocker)

        let snapshot = fixture.root.appendingPathComponent("snapshot")
        assertSuccess(HvfWindowsKernelPolicyVerifier.stageVerifiedSnapshot(
            from: fixture.package, to: snapshot, now: fixture.now,
            trustAnchors: [fixture.anchor]))
        assertSuccess(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: snapshot, now: fixture.now,
            trustAnchors: [fixture.anchor]))

        let poisoned = fixture.root.appendingPathComponent("poisoned")
        let copy: (URL, URL) throws -> Void = { source, destination in
            if source.lastPathComponent == "viogpu3d.sys" {
                try Data("replacement".utf8).append(to: source)
            }
            try FileManager.default.copyItem(at: source, to: destination)
        }
        assertFailure(HvfWindowsKernelPolicyVerifier.stageVerifiedSnapshot(
            from: fixture.package, to: poisoned, now: fixture.now,
            trustAnchors: [fixture.anchor], copyFile: copy), .snapshotInvalid)
        XCTAssertFalse(FileManager.default.fileExists(atPath: poisoned.path))
    }

    func testSignatureTrustAndTimeMutationsFailClosed() throws {
        let signatureFixture = try makeFixture()
        let signature = signatureFixture.package.appendingPathComponent(
            HvfWindowsKernelPolicyVerifier.signatureName)
        var bytes = try Data(contentsOf: signature); bytes[0] ^= 0x80; try bytes.write(to: signature)
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: signatureFixture.package, now: signatureFixture.now,
            trustAnchors: [signatureFixture.anchor]), .signatureInvalid)

        let keyFixture = try makeFixture()
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: keyFixture.package, now: keyFixture.now,
            trustAnchors: []), .trustAnchorUnknown)
        let revoked = HvfWindowsKernelPolicyVerifier.TrustAnchor(
            keyID: keyFixture.anchor.keyID,
            publicKeyBase64: keyFixture.anchor.publicKeyBase64,
            notBefore: keyFixture.anchor.notBefore,
            notAfter: keyFixture.anchor.notAfter,
            revoked: true)
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: keyFixture.package, now: keyFixture.now,
            trustAnchors: [revoked]), .trustAnchorRevoked)

        let timeFixture = try makeFixture()
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: timeFixture.package,
            now: try date("2026-08-27T00:00:00Z"),
            trustAnchors: [timeFixture.anchor]), .timeInvalid)
    }

    func testCanonicalInventoryHashAndReportMutationsFailClosed() throws {
        let canonicalFixture = try makeFixture()
        let manifest = canonicalFixture.package.appendingPathComponent(
            HvfWindowsKernelPolicyVerifier.attestationName)
        var text = try String(contentsOf: manifest, encoding: .ascii)
        text.replaceSubrange(text.startIndex...text.startIndex, with: "{\"unknown\":true,")
        try text.write(to: manifest, atomically: true, encoding: .ascii)
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: canonicalFixture.package, now: canonicalFixture.now,
            trustAnchors: [canonicalFixture.anchor]), .attestationInvalid)

        let hashFixture = try makeFixture()
        try Data("tamper".utf8).append(to: hashFixture.package.appendingPathComponent("viogpu3d.sys"))
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: hashFixture.package, now: hashFixture.now,
            trustAnchors: [hashFixture.anchor]), .hashMismatch)

        let inventoryFixture = try makeFixture()
        try Data().write(to: inventoryFixture.package.appendingPathComponent("extra.dll"))
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: inventoryFixture.package, now: inventoryFixture.now,
            trustAnchors: [inventoryFixture.anchor]), .inventoryInvalid)

        let symlinkFixture = try makeFixture()
        try FileManager.default.createSymbolicLink(
            at: symlinkFixture.package.appendingPathComponent("linked.dll"),
            withDestinationURL: symlinkFixture.package.appendingPathComponent("viogpu3d.sys"))
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: symlinkFixture.package, now: symlinkFixture.now,
            trustAnchors: [symlinkFixture.anchor]), .inventoryInvalid)

        let reportFixture = try makeFixture(validReportPolicy: false)
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: reportFixture.package, now: reportFixture.now,
            trustAnchors: [reportFixture.anchor]), .reportPolicyInvalid)

        let duplicateFixture = try makeFixture(duplicateReportField: true)
        XCTAssertEqual(HvfWindowsKernelPolicyVerifier.inspectReport(
            packageDirectory: duplicateFixture.package).blocker, "signing-report-invalid")
        assertFailure(HvfWindowsKernelPolicyVerifier.verify(
            packageDirectory: duplicateFixture.package, now: duplicateFixture.now,
            trustAnchors: [duplicateFixture.anchor]), .reportPolicyInvalid)
    }

    struct Fixture {
        let root: URL
        let package: URL
        let anchor: HvfWindowsKernelPolicyVerifier.TrustAnchor
        let now: Date
    }

    func makeFixture(
        validReportPolicy: Bool = true,
        duplicateReportField: Bool = false
    ) throws -> Fixture {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let package = root.appendingPathComponent("package", isDirectory: true)
        try FileManager.default.createDirectory(at: package, withIntermediateDirectories: true)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: root.path)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let payloads = [
            "viogpu3d.cat": Data("catalog".utf8),
            "viogpu3d.inf": Data("inf".utf8),
            "viogpu3d.sys": Data("system".utf8),
        ]
        for (name, data) in payloads {
            try data.write(to: package.appendingPathComponent(name))
        }
        var report = """
        BridgeVM viogpu3d Windows WDK finalization
        finalization_complete=true
        signing_mode=kernel-policy
        test_signing_required=false
        sys_kernel_policy_verified=true
        cat_kernel_policy_verified=\(validReportPolicy ? "true" : "false")

        """
        if duplicateReportField { report += "signing_mode=kernel-policy\n" }
        for name in payloads.keys.sorted() {
            report += "sha256.\(name)=\(hash(payloads[name]!))\n"
        }
        try report.write(
            to: package.appendingPathComponent(HvfWindowsKernelPolicyVerifier.reportName),
            atomically: true, encoding: .ascii)

        let artifactNames = (Array(payloads.keys) + [HvfWindowsKernelPolicyVerifier.reportName]).sorted()
        let artifacts = try artifactNames.map { name in
            let data = try Data(contentsOf: package.appendingPathComponent(name))
            return HvfWindowsKernelPolicyVerifier.Artifact(fileName: name, sha256: hash(data))
        }
        let key = try Curve25519.Signing.PrivateKey(
            rawRepresentation: Data((1...32).map(UInt8.init)))
        let attestation = HvfWindowsKernelPolicyVerifier.Attestation(
            artifacts: artifacts, expiresAt: expiresAt, issuedAt: issuedAt,
            keyID: "test-key", packageID: "test-package",
            policy: "windows-kernel-policy", schemaVersion: 1)
        let manifestData = HvfWindowsKernelPolicyVerifier.canonicalData(attestation)
        try manifestData.write(to: package.appendingPathComponent(
            HvfWindowsKernelPolicyVerifier.attestationName))
        try key.signature(for: manifestData).write(to: package.appendingPathComponent(
            HvfWindowsKernelPolicyVerifier.signatureName))
        let anchor = HvfWindowsKernelPolicyVerifier.TrustAnchor(
            keyID: "test-key",
            publicKeyBase64: key.publicKey.rawRepresentation.base64EncodedString(),
            notBefore: "2026-08-24T00:00:00Z", notAfter: "2026-08-27T00:00:00Z",
            revoked: false)
        return Fixture(root: root, package: package, anchor: anchor,
                       now: try date("2026-08-25T12:00:00Z"))
    }

    func assertSuccess(
        _ result: Result<HvfWindowsKernelPolicyVerifier.VerifiedPackage,
            HvfWindowsKernelPolicyVerifier.Failure>,
        file: StaticString = #filePath, line: UInt = #line
    ) {
        if case .failure(let failure) = result {
            XCTFail("unexpected verification failure: \(failure)", file: file, line: line)
        }
    }

    func assertFailure(
        _ result: Result<HvfWindowsKernelPolicyVerifier.VerifiedPackage,
            HvfWindowsKernelPolicyVerifier.Failure>,
        _ expected: HvfWindowsKernelPolicyVerifier.Failure,
        file: StaticString = #filePath, line: UInt = #line
    ) {
        guard case .failure(let failure) = result else {
            XCTFail("mutation unexpectedly verified", file: file, line: line); return
        }
        XCTAssertEqual(failure, expected, file: file, line: line)
    }

    private func hash(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private func date(_ text: String) throws -> Date {
        let formatter = ISO8601DateFormatter()
        return try XCTUnwrap(formatter.date(from: text))
    }
}

private extension Data {
    func append(to url: URL) throws {
        let handle = try FileHandle(forWritingTo: url)
        try handle.seekToEnd(); try handle.write(contentsOf: self); try handle.close()
    }
}
