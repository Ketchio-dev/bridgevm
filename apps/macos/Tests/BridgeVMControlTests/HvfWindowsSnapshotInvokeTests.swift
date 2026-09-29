import XCTest
@testable import BridgeVMControl

/// Pipe-capacity contracts for the snapshot helper call. Each call runs behind a
/// watchdog, so a helper blocked on a full pipe fails a test instead of the suite.
final class HvfWindowsSnapshotInvokeTests: XCTestCase {
    func testOutputBeyondPipeCapacityIsDrainedAndReturned() throws {
        let output = try XCTUnwrap(invokeShell("head -c 200000 /dev/zero | tr '\\0' x"), Self.blocked).get()
        XCTAssertEqual(output.utf8.count, 200_000)
        XCTAssertTrue(output.utf8.allSatisfy { $0 == UInt8(ascii: "x") })
    }

    func testLargeFailureBelowTheCapKeepsItsFinalError() throws {
        let result = try XCTUnwrap(invokeShell(
            "head -c 200000 /dev/zero | tr '\\0' x; printf 'FINAL_REFUSAL\\n' >&2; exit 9"), Self.blocked)
        XCTAssertThrowsError(try result.get()) { error in
            XCTAssertEqual((error as NSError).domain, "BridgeVM.HvfWindowsSnapshot")
            XCTAssertTrue(error.localizedDescription.hasSuffix("FINAL_REFUSAL"))
            XCTAssertFalse(error.localizedDescription.contains("exceeded"))
        }
    }

    func testOutputAboveTheCapFailsClosedDespiteExitZero() throws {
        let result = try XCTUnwrap(invokeShell(
            "head -c 4194304 /dev/zero | tr '\\0' x; printf OVERFLOW_END; exit 0"), Self.blocked)
        XCTAssertThrowsError(try result.get(), "truncated output must not become a success") { error in
            let message = error.localizedDescription
            XCTAssertEqual((error as NSError).domain, "BridgeVM.HvfWindowsSnapshot")
            XCTAssertTrue(message.contains("exceeded"), String(message.prefix(200)))
            XCTAssertTrue(message.hasSuffix("OVERFLOW_END"))
            XCTAssertLessThan(message.utf8.count, 64 * 1024, "the error is a diagnostic, not a transcript")
        }
    }

    func testOverflowingFailureKeepsItsFinalError() throws {
        let result = try XCTUnwrap(invokeShell(
            "head -c 4194304 /dev/zero | tr '\\0' x; printf 'FINAL_REFUSAL\\n' >&2; exit 23"), Self.blocked)
        XCTAssertThrowsError(try result.get()) { error in
            XCTAssertEqual((error as NSError).domain, "BridgeVM.HvfWindowsSnapshot")
            XCTAssertTrue(error.localizedDescription.contains("exceeded"))
            XCTAssertTrue(error.localizedDescription.hasSuffix("FINAL_REFUSAL"))
        }
    }

    func testSmallFailureStillReportsItsOutput() throws {
        let refusal = try XCTUnwrap(invokeShell("printf 'precise refusal\\n' >&2; exit 9"), Self.blocked)
        XCTAssertThrowsError(try refusal.get()) { error in
            XCTAssertEqual((error as NSError).domain, "BridgeVM.HvfWindowsSnapshot")
            XCTAssertEqual(error.localizedDescription, "precise refusal")
        }
        let silent = try XCTUnwrap(invokeShell("exit 7"), Self.blocked)
        XCTAssertThrowsError(try silent.get()) { error in
            XCTAssertEqual(error.localizedDescription, "snapshot operation failed")
        }
    }

    func testSignalTerminationFailsAndSmallSuccessIsExact() throws {
        let killed = try XCTUnwrap(invokeShell("printf partial; kill -KILL $$"), Self.blocked)
        XCTAssertThrowsError(try killed.get())
        let success = try XCTUnwrap(invokeShell("printf 'format_version 1\\n'"), Self.blocked)
        XCTAssertEqual(try success.get(), "format_version 1\n")
    }

    private static let blocked = "invoke did not return: the helper is blocked on a full pipe"

    /// nil when invoke is still blocked at the deadline. The blocked child is left
    /// behind and dies of a broken pipe when this test process exits.
    private func invokeShell(_ script: String, seconds: Int = 15) -> Result<String, Error>? {
        let outcome = Outcome()
        let done = DispatchSemaphore(value: 0)
        Thread.detachNewThread {
            outcome.set(Result { try HvfWindowsSnapshotCommand.invoke(URL(fileURLWithPath: "/bin/sh"), ["-c", script]) })
            done.signal()
        }
        guard done.wait(timeout: .now() + .seconds(seconds)) == .success else { return nil }
        return outcome.get()
    }

    private final class Outcome: @unchecked Sendable {
        private let lock = NSLock()
        private var value: Result<String, Error>?

        func set(_ result: Result<String, Error>) {
            lock.lock(); defer { lock.unlock() }
            value = result
        }

        func get() -> Result<String, Error>? {
            lock.lock(); defer { lock.unlock() }
            return value
        }
    }
}
