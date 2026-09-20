import XCTest
@testable import BridgeVMProductE2E

final class T17CreatedVMIdentityTests: XCTestCase {
    private let expected: [String: Any] = [
        "name": "BridgeVM T17 Lane 1 abc", "id": "bridgevm-t17-lane-1-abc",
        "installPending": true, "bundlePath": "/tmp/lane/library/vm/bundle.vmbridge",
    ]

    func testExactIdentityHasNoMismatch() throws {
        XCTAssertEqual(mismatches(expected), [])
        XCTAssertNoThrow(try verify(expected))
    }

    func testEachAllowListedMismatchIsAttributedInStableOrder() throws {
        let replacements: [(String, Any)] = [
            ("name", "private-name"), ("id", "private-id"),
            ("installPending", false), ("bundlePath", "/private/value"),
        ]
        for (field, value) in replacements {
            var object = expected; object[field] = value
            XCTAssertEqual(mismatches(object), [field])
        }
        XCTAssertEqual(mismatches([:]), ["name", "id", "installPending", "bundlePath"])
    }

    func testFailureContainsOnlyFieldNames() throws {
        var object = expected; object["name"] = "secret"; object["bundlePath"] = "/secret"
        XCTAssertThrowsError(try verify(object)) { error in
            XCTAssertEqual(error as? T17Blocker, T17Blocker(
                code: "vm-creation-failed",
                detail: "UI-created VM config identity mismatch: name,bundlePath"))
            XCTAssertFalse(String(describing: error).contains("secret"))
        }
    }

    private func mismatches(_ object: [String: Any]) -> [String] {
        T17CreatedVMIdentity.mismatches(in: object, name: expected["name"] as! String,
            id: expected["id"] as! String, bundlePath: expected["bundlePath"] as! String)
    }

    private func verify(_ object: [String: Any]) throws {
        try T17CreatedVMIdentity.verify(object, name: expected["name"] as! String,
            id: expected["id"] as! String, bundlePath: expected["bundlePath"] as! String)
    }
}
