import XCTest
@testable import BridgeVMProductE2E

/// failure_code alone cost a day: "ui-element-missing" says nothing about which
/// element. The detail the blocker was thrown with now rides beside the code.
final class T17LaneDetailTests: XCTestCase {
    func testLaneResultCarriesTheBlockerDetailBesideItsCode() throws {
        let fixture = try T17ContractTests().makeFixture()
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
