import XCTest
@testable import BridgeVMProductE2E

final class T17ChooserNativeActivationTests: XCTestCase {
    func testLateInactiveReadDoesNotActivate() {
        var time = 0.0, activations = 0
        let result = measure(time: { time }, active: { time = 1; return false },
            activate: { activations += 1; return true })
        XCTAssertFalse(result.succeeded); XCTAssertEqual(activations, 0)
    }
    func testLateActivationDoesNotWriteAXFrontmost() {
        var time = 0.0, writes = 0
        let result = measure(time: { time }, activate: { time = 1; return true },
            setFront: { writes += 1; return 0 })
        XCTAssertFalse(result.succeeded); XCTAssertEqual(writes, 0)
    }
    func testLateAXWriteDoesNotPauseOrRead() {
        var time = 0.0, pauses = 0, reads = 0
        let result = measure(time: { time }, setFront: { time = 1; return 0 },
            pause: { _ in pauses += 1 }, readFront: { reads += 1; return (0, true) })
        XCTAssertFalse(result.succeeded); XCTAssertEqual(pauses, 0); XCTAssertEqual(reads, 0)
    }
    func testLatePauseDoesNotReadAXFrontmost() {
        var time = 0.0, reads = 0
        let result = measure(time: { time }, pause: { _ in time = 1 },
            readFront: { reads += 1; return (0, true) })
        XCTAssertFalse(result.succeeded); XCTAssertEqual(reads, 0)
    }
    func testAlreadyActiveWithinBudgetNeedsNoInput() {
        let result = measure(time: { 0 }, active: { true }, activate: { XCTFail("unexpected activation"); return true },
            setFront: { XCTFail("unexpected AX write"); return 0 })
        XCTAssertTrue(result.succeeded); XCTAssertEqual(result.attempts, 0)
    }
    private func measure(time: () -> TimeInterval, active: () -> Bool? = { false },
                         activate: () -> Bool? = { true }, setFront: () -> Int32 = { 0 },
                         pause: (TimeInterval) -> Void = { _ in },
                         readFront: () -> (Int32, Bool?) = { (0, false) }) -> T17ActivationRecord {
        T17ActivationProbe.measure(timeout: 5, clock: time, isActive: active, activate: activate,
            setFront: setFront, readFront: readFront, pause: pause, foreground: { 7 }, admitted: { time() < 1 })
    }
}
