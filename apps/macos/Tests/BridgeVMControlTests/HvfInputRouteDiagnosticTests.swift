import XCTest
@testable import BridgeVMControl

final class HvfInputRouteDiagnosticTests: XCTestCase {
    func testLabelsUseOnlyCharactersTheT17DisplayDiagnosticKeeps() {
        let label = HvfInputRouteDiagnostic.label(owned: false, state: .waitingForLegacy, activated: false, failed: true, attached: true)
        XCTAssertEqual(label, "unowned-waitingForLegacy-inactive-failed-attached")
        XCTAssertTrue(label.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-") })
        XCTAssertEqual(HvfInputRouteDiagnostic.label(owned: true, state: .ready, activated: true, failed: false, attached: false),
                       "owned-ready-active")
    }
    func testRouterDiagnosticFollowsOwnershipTransitions() {
        var router = HvfSessionInputRouter()
        XCTAssertEqual(router.routeDiagnostic(attached: false), "unowned-disconnected-inactive")
        router.beginOwnedBoot(binding: ["disk"])
        XCTAssertEqual(router.routeDiagnostic(attached: false), "owned-disconnected-inactive")
        router.attachUnknown(binding: ["disk"])
        XCTAssertEqual(router.routeDiagnostic(attached: true), "unowned-disconnected-inactive-attached")
    }
}
