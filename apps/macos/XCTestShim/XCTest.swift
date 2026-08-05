// A stand-in for the XCTest API this repository uses.
//
// Not Apple's XCTest. Results measured with it must say so. It exists because
// XCTest.framework ships with Xcode, Xcode requires an Apple ID to download,
// and 651 test functions are otherwise unreachable on this machine.
//
// The rule this file has to obey is narrow and absolute: an assertion that
// should fail must fail. A shim that quietly passes everything is worse than
// no shim, because it converts unknown state into false confidence.
// Re-exported: Apple's XCTest re-exports Foundation, and hundreds of tests
// rely on `import XCTest` alone bringing in Date, TimeInterval, DispatchQueue.
@_exported import Foundation
@_exported import Dispatch

public struct XCTestFailure: Error, CustomStringConvertible {
    public let message: String
    public var description: String { message }
}

/// Thrown by XCTSkip; a skipped test is not a passing test.
public struct XCTSkipError: Error {
    public let message: String
}

/// Collected per test method by the runner, which resets it between methods.
public final class XCTestRecorder: @unchecked Sendable {
    public static let shared = XCTestRecorder()
    private let lock = NSLock()
    private var failures: [String] = []

    public func record(_ message: String) {
        lock.lock(); defer { lock.unlock() }
        failures.append(message)
    }

    public func drain() -> [String] {
        lock.lock(); defer { lock.unlock() }
        let out = failures
        failures = []
        return out
    }
}

private func fail(_ what: String, _ detail: String, _ message: String,
                  _ file: StaticString, _ line: UInt) {
    let suffix = message.isEmpty ? "" : " - \(message)"
    XCTestRecorder.shared.record("\(file):\(line) \(what) failed: \(detail)\(suffix)")
}

/// Apple's XCTAssert* are not `rethrows`. They evaluate a throwing autoclosure
/// and record a thrown error as a failure, so callers write `try` inside the
/// call without marking the call site. Matching that is the whole point: a
/// shim that needs the tests edited is not testing the same code.
private func evaluate<T>(_ what: String, _ expression: () throws -> T,
                         _ message: String, _ file: StaticString,
                         _ line: UInt) -> T? {
    do { return try expression() } catch {
        fail(what, "threw \(error)", message, file, line)
        return nil
    }
}

open class XCTestCase {
    public required init() {}
    open func setUp() {}
    open func setUpWithError() throws {}
    open func tearDown() {}
    open func tearDownWithError() throws {}

    private var teardownBlocks: [() -> Void] = []
    public func addTeardownBlock(_ block: @escaping () -> Void) {
        teardownBlocks.append(block)
    }
    public func runTeardownBlocks() {
        for block in teardownBlocks.reversed() { block() }
        teardownBlocks = []
    }
}

public func XCTAssertEqual<T: Equatable>(
    _ lhs: @autoclosure () throws -> T, _ rhs: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let a = evaluate("XCTAssertEqual", lhs, message(), file, line),
          let b = evaluate("XCTAssertEqual", rhs, message(), file, line) else { return }
    if a != b { fail("XCTAssertEqual", "\(a) != \(b)", message(), file, line) }
}

/// Floating-point comparison with a tolerance, as XCTest spells it.
public func XCTAssertEqual<T: FloatingPoint>(
    _ lhs: @autoclosure () throws -> T, _ rhs: @autoclosure () throws -> T,
    accuracy: T, _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let a = evaluate("XCTAssertEqual", lhs, message(), file, line),
          let b = evaluate("XCTAssertEqual", rhs, message(), file, line) else { return }
    if !(abs(a - b) <= accuracy) {
        fail("XCTAssertEqual", "\(a) and \(b) differ by more than \(accuracy)", message(), file, line)
    }
}

public func XCTAssertNotEqual<T: Equatable>(
    _ lhs: @autoclosure () throws -> T, _ rhs: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let a = evaluate("XCTAssertNotEqual", lhs, message(), file, line),
          let b = evaluate("XCTAssertNotEqual", rhs, message(), file, line) else { return }
    if a == b { fail("XCTAssertNotEqual", "both \(a)", message(), file, line) }
}

public func XCTAssertTrue(
    _ expression: @autoclosure () throws -> Bool,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    if evaluate("XCTAssertTrue", expression, message(), file, line) == false {
        fail("XCTAssertTrue", "expression is false", message(), file, line)
    }
}

public func XCTAssertFalse(
    _ expression: @autoclosure () throws -> Bool,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    if evaluate("XCTAssertFalse", expression, message(), file, line) == true {
        fail("XCTAssertFalse", "expression is true", message(), file, line)
    }
}

public func XCTAssertNil(
    _ expression: @autoclosure () throws -> Any?,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let outer = evaluate("XCTAssertNil", expression, message(), file, line) else { return }
    if let value = outer { fail("XCTAssertNil", "got \(value)", message(), file, line) }
}

public func XCTAssertNotNil(
    _ expression: @autoclosure () throws -> Any?,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let outer = evaluate("XCTAssertNotNil", expression, message(), file, line) else { return }
    if outer == nil { fail("XCTAssertNotNil", "got nil", message(), file, line) }
}

public func XCTAssertLessThan<T: Comparable>(
    _ lhs: @autoclosure () throws -> T, _ rhs: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let a = evaluate("XCTAssertLessThan", lhs, message(), file, line),
          let b = evaluate("XCTAssertLessThan", rhs, message(), file, line) else { return }
    if !(a < b) { fail("XCTAssertLessThan", "\(a) is not < \(b)", message(), file, line) }
}

public func XCTAssertGreaterThan<T: Comparable>(
    _ lhs: @autoclosure () throws -> T, _ rhs: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let a = evaluate("XCTAssertGreaterThan", lhs, message(), file, line),
          let b = evaluate("XCTAssertGreaterThan", rhs, message(), file, line) else { return }
    if !(a > b) { fail("XCTAssertGreaterThan", "\(a) is not > \(b)", message(), file, line) }
}

public func XCTAssertGreaterThanOrEqual<T: Comparable>(
    _ lhs: @autoclosure () throws -> T, _ rhs: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    guard let a = evaluate("XCTAssertGreaterThanOrEqual", lhs, message(), file, line),
          let b = evaluate("XCTAssertGreaterThanOrEqual", rhs, message(), file, line) else { return }
    if !(a >= b) { fail("XCTAssertGreaterThanOrEqual", "\(a) is not >= \(b)", message(), file, line) }
}

public func XCTAssertThrowsError<T>(
    _ expression: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line,
    _ errorHandler: (Error) -> Void = { _ in }
) {
    do {
        _ = try expression()
        fail("XCTAssertThrowsError", "no error thrown", message(), file, line)
    } catch {
        errorHandler(error)
    }
}

public func XCTAssertNoThrow<T>(
    _ expression: @autoclosure () throws -> T,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    do { _ = try expression() } catch {
        fail("XCTAssertNoThrow", "threw \(error)", message(), file, line)
    }
}

public func XCTUnwrap<T>(
    _ expression: @autoclosure () throws -> T?,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) throws -> T {
    guard let value = try expression() else {
        let suffix = message().isEmpty ? "" : " - \(message())"
        let text = "\(file):\(line) XCTUnwrap failed: value is nil\(suffix)"
        XCTestRecorder.shared.record(text)
        throw XCTestFailure(message: text)
    }
    return value
}

public func XCTFail(
    _ message: String = "",
    file: StaticString = #filePath, line: UInt = #line
) {
    fail("XCTFail", "called", message, file, line)
}

public func XCTSkip(
    _ message: String = "",
    file: StaticString = #filePath, line: UInt = #line
) -> XCTSkipError {
    XCTSkipError(message: "\(file):\(line) skipped: \(message)")
}

public func XCTSkipIf(
    _ condition: @autoclosure () throws -> Bool,
    _ message: @autoclosure () -> String = "",
    file: StaticString = #filePath, line: UInt = #line
) throws {
    if try condition() { throw XCTSkip(message(), file: file, line: line) }
}
