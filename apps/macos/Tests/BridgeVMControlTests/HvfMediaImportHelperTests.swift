import XCTest
@testable import BridgeVMControl

/// Pipe-capacity and output-limit contracts for the native import helper call,
/// driven through a stand-in helper. Each call runs behind a watchdog, so a
/// helper blocked on a full pipe fails a test instead of the suite.
final class HvfMediaImportHelperTests: XCTestCase {
    static let blocked = "invoke did not return: the helper is blocked on a full pipe"

    func testOutputBeyondPipeCapacityIsDrainedAndSucceeds() throws {
        let result = try XCTUnwrap(try invokeStandIn("head -c 200000 /dev/zero | tr '\\0' x"), Self.blocked)
        XCTAssertNoThrow(try result.get())
    }

    func testFloodedFailureReportsADiagnosticNotATranscript() throws {
        let result = try XCTUnwrap(try invokeStandIn(
            "head -c 4194304 /dev/zero | tr '\\0' x; printf 'FINAL_REFUSAL\\n' >&2; exit 23"), Self.blocked)
        XCTAssertThrowsError(try result.get()) { error in
            let message = error.localizedDescription
            XCTAssertEqual((error as NSError).domain, "BridgeVM.MediaImport")
            XCTAssertTrue(message.hasSuffix("FINAL_REFUSAL"), String(message.suffix(200)))
            XCTAssertLessThan(message.utf8.count, 64 * 1024, "the error is a diagnostic, not a transcript")
        }
    }

    func testOutputAboveTheCapFailsClosedDespiteExitZero() throws {
        let result = try XCTUnwrap(try invokeStandIn("head -c 4194304 /dev/zero | tr '\\0' x; exit 0"), Self.blocked)
        XCTAssertThrowsError(try result.get(), "a helper past the output limit must not pass as an import") { error in
            XCTAssertEqual((error as NSError).domain, "BridgeVM.MediaImport")
            XCTAssertTrue(error.localizedDescription.hasPrefix("helper output exceeded 1048576 bytes"),
                          String(error.localizedDescription.prefix(200)))
        }
    }

    func testSmallFailuresKeepTheirMessageAndSignalsFail() throws {
        let refusal = try XCTUnwrap(try invokeStandIn("printf 'precise refusal\\n' >&2; exit 9"), Self.blocked)
        XCTAssertThrowsError(try refusal.get()) { error in
            XCTAssertEqual(error.localizedDescription, "precise refusal")
        }
        let silent = try XCTUnwrap(try invokeStandIn("exit 7"), Self.blocked)
        XCTAssertThrowsError(try silent.get()) { error in
            XCTAssertEqual(error.localizedDescription, "디스크와 부팅 설정을 가져오지 못했습니다.")
        }
        let killed = try XCTUnwrap(try invokeStandIn("printf 'format_version 1\\n'; kill -KILL $$"), Self.blocked)
        XCTAssertThrowsError(try killed.get())
    }

    /// nil when invoke is still blocked at the deadline. The blocked child is left
    /// behind and dies of a broken pipe when this test process exits.
    private func invokeStandIn(_ script: String, seconds: Int = 15) throws -> Result<Void, Error>? {
        let directory = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("hvf-import-helper-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: directory) }
        let helper = directory.appendingPathComponent("snapshot_pair_cli")
        try Data("#!/bin/sh\n\(script)\n".utf8).write(to: helper)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: helper.path)
        let outcome = Outcome()
        let done = DispatchSemaphore(value: 0)
        Thread.detachNewThread {
            outcome.set(Result { try HvfMediaImportHelper.invoke(helper, arguments: []) })
            done.signal()
        }
        guard done.wait(timeout: .now() + .seconds(seconds)) == .success else { return nil }
        return outcome.get()
    }

    private final class Outcome: @unchecked Sendable {
        private let lock = NSLock()
        private var value: Result<Void, Error>?

        func set(_ result: Result<Void, Error>) {
            lock.lock(); defer { lock.unlock() }
            value = result
        }

        func get() -> Result<Void, Error>? {
            lock.lock(); defer { lock.unlock() }
            return value
        }
    }
}
