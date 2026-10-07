import XCTest
@testable import BridgeVMProductE2E

/// Retain the specific failing element beside the generic failure code.
final class T17LaneDetailTests: XCTestCase {
    func testLaneResultCarriesTheBlockerDetailBesideItsCode() throws {
        let owner = T17ContractTests(); defer { owner.tearDown() }
        let fixture = try owner.makeFixture()
        let request = try T17Request.load(fixture.request)
        var evidence = T17Evidence(nonce: request.nonce)
        try evidence.prove(.artifactPreflight)
        let detailed = evidence.result(
            request: request, failureCode: "ui-element-missing",
            failureDetail: "required accessibility identifier was not found: bridgevm.x; windows=1",
            cleanupVerified: true, installerSourcePath: "absent", uiFrontendAutomated: true)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(detailed)) as? [String: Any])
        XCTAssertEqual(object["failure_code"] as? String, "ui-element-missing")
        XCTAssertEqual(object["failure_detail"] as? String,
                       "required accessibility identifier was not found: bridgevm.x; windows=1")
        let bare = evidence.result(
            request: request, failureCode: "accessibility-untrusted",
            cleanupVerified: true, installerSourcePath: "absent", uiFrontendAutomated: false)
        let bareObject = try XCTUnwrap(JSONSerialization.jsonObject(
            with: JSONEncoder().encode(bare)) as? [String: Any])
        XCTAssertEqual(bareObject["failure_detail"] as? String, "")
    }
}
