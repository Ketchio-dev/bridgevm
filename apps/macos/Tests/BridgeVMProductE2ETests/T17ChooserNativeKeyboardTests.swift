import ApplicationServices
import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserNativeKeyboardTests: XCTestCase {
    func testLateActivationDoesNotPostPair() {
        var time = 0.0, pairs = 0, foregroundReads = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17FileChooserKeyboard.deliver(pid: 7,
            activate: { time = 1; return true }, foreground: { foregroundReads += 1; return 7 },
            admission: { try budget.check(.keyboard) }, postPair: { pairs += 1 }))
        XCTAssertEqual(pairs, 0); XCTAssertEqual(foregroundReads, 0)
    }
    func testLateForegroundReadDoesNotPostPair() {
        var time = 0.0, pairs = 0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try T17FileChooserKeyboard.deliver(pid: 7, activate: { true },
            foreground: { time = 1; return 7 }, admission: { try budget.check(.keyboard) },
            postPair: { pairs += 1 }))
        XCTAssertEqual(pairs, 0)
    }
    func testAdmittedPairCompletesBothEventsOnceEvenWhenItExpires() {
        var time = 0.0, events: [String] = []
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        var activation = T17ActivationRecord(); activation.succeeded = true
        XCTAssertThrowsError(try T17FileChooserKeyDiagnostic.run(pid: 7, code: 36, flags: [],
            activate: { activation }, foreground: { 7 }, postPair: {
                events.append("down"); time = 1; events.append("up")
            }, admission: { try budget.check(.keyboard) })) { error in
                let detail = (error as? T17Blocker)?.detail ?? ""
                XCTAssertTrue(detail.contains("operation=keyboard-pair"))
                XCTAssertTrue(detail.contains("pair_posted=true"))
            }
        XCTAssertEqual(events, ["down", "up"])
    }
    func testNestedReadExpiryKeepsSelectionBoundaryLabel() {
        var time = 0.0
        let budget = T17ChooserNativeBudget(deadline: 1, now: { time })
        XCTAssertThrowsError(try budget.read(.buttonLookup) {
            try budget.read(.relationship) { time = 1; return 7 }
        }) { error in
            let detail = (error as? T17Blocker)?.detail ?? ""
            XCTAssertTrue(detail.contains("operation=selection-button-lookup"))
            XCTAssertTrue(detail.contains("operation=ax-relationship"))
            XCTAssertTrue(detail.count <= 340)
        }
    }
}
