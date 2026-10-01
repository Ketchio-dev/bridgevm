import XCTest
@testable import BridgeVMProductE2E

final class T17DisplayInputDiagnosticTests: XCTestCase {
    func testAFailedClickCarriesTheDisplaysOwnRecord() throws {
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("t17-click-\(UUID().uuidString).log")
        try Data("BVAGENT SERVICE alive t=1\n".utf8).write(to: log)
        defer { try? FileManager.default.removeItem(at: log) }
        XCTAssertThrowsError(try T17PointerReceipt.require(log, timeout: 0.2, diagnostic: { "; display=presses=0" }) {}) { error in
            XCTAssertEqual((error as? T17Blocker)?.detail,
                           "display click was not inserted as guest pointer input; display=presses=0")
        }
    }

    func testOnlyTheAppVocabularySurvivesAndStaysBounded() {
        XCTAssertEqual(T17DisplayInputDiagnostic.sanitized("presses=2 last=refused"), "presses=2 last=refused")
        XCTAssertEqual(T17DisplayInputDiagnostic.sanitized("presses=1;\nhost_stop=status=complete"), "presses=1hoststop=status=complete")
        XCTAssertEqual(T17DisplayInputDiagnostic.sanitized(String(repeating: "a", count: 200)).count, 80)
    }
}
