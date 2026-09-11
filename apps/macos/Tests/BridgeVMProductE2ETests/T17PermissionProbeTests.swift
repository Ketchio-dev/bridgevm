import XCTest
@testable import BridgeVMProductE2E

final class T17PermissionProbeTests: XCTestCase {
    func testProbeModeRequiresTheExactStandaloneArgument() throws {
        XCTAssertTrue(T17PermissionProbe.accepts(["--accessibility-diagnostic"]))
        for arguments in [[], ["--help"], ["--accessibility-diagnostic", "--request", "/tmp/request"],
                          ["--accessibility-diagnostic", "--accessibility-diagnostic"]] {
            XCTAssertFalse(T17PermissionProbe.accepts(arguments))
            XCTAssertFalse(try T17PermissionProbe.run(arguments))
        }
    }

    func testUntrustedObservationDoesNotBecomeAProductPass() throws {
        let report = try object(trusted: false)
        XCTAssertEqual(report["schema"] as? String, "t17.accessibility-diagnostic.v1")
        XCTAssertEqual(report["accessibility_trusted"] as? Bool, false)
        XCTAssertEqual(report["observation_only"] as? Bool, true)
        XCTAssertEqual(report["criterion_pass"] as? Bool, false)
    }

    func testTrustedObservationStillDoesNotBecomeAProductPass() throws {
        let report = try object(trusted: true)
        XCTAssertEqual(report["accessibility_trusted"] as? Bool, true)
        XCTAssertEqual(report["criterion_pass"] as? Bool, false)
        XCTAssertEqual(report["caller_identity"] as? [String: String], ["pid": "42"])
        XCTAssertEqual(report["scope"] as? String, "calling-process-only-not-product-e2e-or-tcc-database-attribution")
    }

    private func object(trusted: Bool) throws -> [String: Any] {
        let data = try T17PermissionProbe.encode(identity: ["pid": "42"], trusted: trusted)
        XCTAssertEqual(data.last, 10)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }
}
