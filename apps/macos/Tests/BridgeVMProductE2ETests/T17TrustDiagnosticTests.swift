import Security
import XCTest
@testable import BridgeVMProductE2E

final class T17TrustDiagnosticTests: XCTestCase {
    func testPathDigestIsStableAndDoesNotRevealThePath() {
        let path = "/private/example/first/Helper.app"
        let digest = T17TrustDiagnostic.pathDigest(path)
        XCTAssertEqual(digest.count, 64)
        XCTAssertEqual(digest, T17TrustDiagnostic.pathDigest(path))
        XCTAssertNotEqual(digest, T17TrustDiagnostic.pathDigest(path + "/other"))
        XCTAssertFalse(digest.contains("private"))
    }

    func testUnsignedOrMalformedMetadataRemainsUnavailable() {
        XCTAssertEqual(T17TrustDiagnostic.signingFields([:])["code_cdhash"], "unavailable")
        let malformed: [String: Any] = [kSecCodeInfoUnique as String: "not-data",
                                       kSecCodeInfoIdentifier as String: 42]
        XCTAssertEqual(T17TrustDiagnostic.signingFields(malformed)["code_identifier"], "unavailable")
        XCTAssertEqual(T17TrustDiagnostic.signingFields(malformed)["code_cdhash"], "unavailable")
    }

    func testSigningMetadataIsBoundedAndHexEncoded() {
        let info: [String: Any] = [kSecCodeInfoIdentifier as String: String(repeating: "a", count: 300),
                                  kSecCodeInfoUnique as String: Data([0, 15, 255])]
        let result = T17TrustDiagnostic.signingFields(info)
        XCTAssertEqual(result["code_identifier"]?.count, 256)
        XCTAssertEqual(result["code_cdhash"], "000fff")
    }

    func testCaptureReportsIdentityWithoutClaimingTrustOrValidity() {
        let fields = T17TrustDiagnostic.capture()
        XCTAssertEqual(fields["schema"], "t17.caller-identity.v1")
        XCTAssertEqual(fields["pid"], String(ProcessInfo.processInfo.processIdentifier))
        XCTAssertEqual(fields["bundle_path_sha256"]?.count, 64)
        XCTAssertNotNil(fields["caller_status"])
        XCTAssertEqual(fields["scope"], "on-disk-code-metadata-not-signature-validation-or-tcc-attribution")
        XCTAssertNil(fields["path"])
        XCTAssertNil(fields["trusted"])
        XCTAssertTrue(T17TrustDiagnostic.detail().hasPrefix("macOS Accessibility permission is not granted; caller_identity={"))
    }
}
