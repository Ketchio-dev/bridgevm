import XCTest
@testable import BridgeVMProductE2E

final class A9ImportSelectedEvidenceTests: XCTestCase {
    func testFinalMediaHashesComeFromTheSelectedPair() throws {
        var evidence = A9ImportEvidence(nonce: String(repeating: "c", count: 64))
        let a = String(repeating: "a", count: 64), b = String(repeating: "b", count: 64)
        try evidence.authenticateSelectedMedia(disk: "/d", vars: "/v") { disk, vars in
            XCTAssertEqual([disk, vars], ["/d", "/v"])
            return .init(diskBytes: 1, diskSHA256: a, varsBytes: 1, varsSHA256: b)
        }
        XCTAssertEqual(evidence.hashes["final_disk_sha256"], a)
        XCTAssertEqual(evidence.hashes["final_vars_sha256"], b)
    }

    func testFailedSelectionLeavesBothFinalHashesUnchanged() throws {
        var evidence = A9ImportEvidence(nonce: String(repeating: "c", count: 64))
        let before = evidence.hashes
        XCTAssertThrowsError(try evidence.authenticateSelectedMedia(disk: "/d", vars: "/v") { _, _ in
            throw T17Blocker(code: "snapshot-unavailable", detail: "injected selection failure")
        })
        XCTAssertEqual(evidence.hashes, before)
    }
}
