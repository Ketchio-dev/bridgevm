import XCTest
@testable import BridgeVMProductE2E

final class T17SelectedEvidenceTests: XCTestCase {
    func testFinalMediaHashesComeFromTheSelectedPair() throws {
        var evidence = T17Evidence(nonce: String(repeating: "c", count: 64))
        let a = String(repeating: "a", count: 64), b = String(repeating: "b", count: 64)
        try evidence.authenticateSelectedMedia(disk: "/d", vars: "/v") { disk, vars in
            XCTAssertEqual([disk, vars], ["/d", "/v"])
            return .init(diskBytes: 1, diskSHA256: a, varsBytes: 1, varsSHA256: b)
        }
        XCTAssertEqual(evidence.hashes["final_disk_sha256"], a)
        XCTAssertEqual(evidence.hashes["final_vars_sha256"], b)
    }
}
