// The shim is only usable if assertions that should fail do fail. Each case
// here is a deliberate falsification: it asserts something untrue and then
// requires the shim to have recorded exactly that.
import XCTest
import Foundation

var checks = 0, bad = 0
func mustFail(_ label: String, _ body: () throws -> Void) rethrows {
    _ = XCTestRecorder.shared.drain()
    try body()
    let failures = XCTestRecorder.shared.drain()
    checks += 1
    if failures.isEmpty { bad += 1; print("NOT CAUGHT: \(label)") }
}
func mustPass(_ label: String, _ body: () throws -> Void) rethrows {
    _ = XCTestRecorder.shared.drain()
    try body()
    let failures = XCTestRecorder.shared.drain()
    checks += 1
    if !failures.isEmpty { bad += 1; print("FALSE ALARM: \(label): \(failures)") }
}

mustFail("XCTAssertEqual on unequal") { XCTAssertEqual(1, 2) }
mustPass("XCTAssertEqual on equal") { XCTAssertEqual(2, 2) }
mustFail("XCTAssertNotEqual on equal") { XCTAssertNotEqual(3, 3) }
mustPass("XCTAssertNotEqual on unequal") { XCTAssertNotEqual(3, 4) }
mustFail("XCTAssertTrue on false") { XCTAssertTrue(false) }
mustPass("XCTAssertTrue on true") { XCTAssertTrue(true) }
mustFail("XCTAssertFalse on true") { XCTAssertFalse(true) }
mustPass("XCTAssertFalse on false") { XCTAssertFalse(false) }
mustFail("XCTAssertNil on value") { XCTAssertNil(Optional<Int>(7)) }
mustPass("XCTAssertNil on nil") { XCTAssertNil(Optional<Int>.none) }
mustFail("XCTAssertNotNil on nil") { XCTAssertNotNil(Optional<Int>.none) }
mustPass("XCTAssertNotNil on value") { XCTAssertNotNil(Optional<Int>(7)) }
mustFail("XCTAssertLessThan when greater") { XCTAssertLessThan(5, 1) }
mustFail("XCTAssertLessThan when equal") { XCTAssertLessThan(5, 5) }
mustPass("XCTAssertLessThan when less") { XCTAssertLessThan(1, 5) }
mustFail("XCTAssertGreaterThan when less") { XCTAssertGreaterThan(1, 5) }
mustPass("XCTAssertGreaterThan when greater") { XCTAssertGreaterThan(5, 1) }
mustFail("XCTAssertGreaterThanOrEqual when less") { XCTAssertGreaterThanOrEqual(1, 5) }
mustPass("XCTAssertGreaterThanOrEqual when equal") { XCTAssertGreaterThanOrEqual(5, 5) }
mustFail("XCTFail") { XCTFail("deliberate") }
mustFail("XCTAssertThrowsError when nothing throws") { XCTAssertThrowsError(1 + 1) }
struct Boom: Error {}
func boom() throws -> Int { throw Boom() }
try mustPass("XCTAssertThrowsError when it throws") { XCTAssertThrowsError(try boom()) }
try mustFail("XCTAssertNoThrow when it throws") { XCTAssertNoThrow(try boom()) }
mustPass("XCTAssertNoThrow when it does not") { XCTAssertNoThrow(1 + 1) }
mustFail("XCTUnwrap on nil") { _ = try? XCTUnwrap(Optional<Int>.none) }
mustPass("XCTUnwrap on value") { _ = try? XCTUnwrap(Optional<Int>(7)) }

// XCTUnwrap must also throw, not just record: tests rely on it stopping.
checks += 1
do {
    _ = try XCTUnwrap(Optional<Int>.none)
    bad += 1; print("NOT CAUGHT: XCTUnwrap did not throw on nil")
} catch {}
_ = XCTestRecorder.shared.drain()

// A skip is not a pass.
checks += 1
do { try XCTSkipIf(true, "because"); bad += 1; print("NOT CAUGHT: XCTSkipIf did not throw") }
catch is XCTSkipError {} catch { bad += 1; print("wrong error type from XCTSkipIf") }

// Teardown blocks run in reverse order, as XCTest does.
checks += 1
final class C: XCTestCase {}
var order: [Int] = []
let c = C(); c.addTeardownBlock { order.append(1) }; c.addTeardownBlock { order.append(2) }
c.runTeardownBlocks()
if order != [2, 1] { bad += 1; print("NOT CAUGHT: teardown order was \(order)") }

print(bad == 0 ? "PASS: XCTest shim self-test (\(checks) checks)"
               : "FAIL: XCTest shim self-test (\(bad) of \(checks) wrong)")
exit(bad == 0 ? 0 : 1)
