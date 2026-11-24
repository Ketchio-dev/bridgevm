import XCTest
@testable import BridgeVMProductE2E

final class T17DisplayInputDiagnosticTests: XCTestCase {
    func testOnlyTheAppVocabularySurvivesAndStaysBounded() {
        XCTAssertEqual(T17DisplayInputDiagnostic.sanitized("presses=2 last=refused"), "presses=2 last=refused")
        XCTAssertEqual(T17DisplayInputDiagnostic.sanitized("presses=1;\nhost_stop=status=complete"), "presses=1hoststop=status=complete")
        XCTAssertEqual(T17DisplayInputDiagnostic.sanitized(String(repeating: "a", count: 200)).count, 80)
    }
}
