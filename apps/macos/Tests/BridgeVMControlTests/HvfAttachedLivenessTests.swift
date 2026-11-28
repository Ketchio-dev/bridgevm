import XCTest
@testable import BridgeVMControl

/// A VM BridgeVM did not start has no Process handle, so its once-a-second liveness lookup reads the
/// disk's writer lock and runs pgrep. poll() runs on the main actor at 10 Hz; the lookup must not.
final class HvfAttachedLivenessTests: XCTestCase {
    @MainActor
    func testTheLookupRunsOffTheMainThreadOneAtATimeAndAStaleAnswerIsIgnored() async throws {
        let temp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: temp) }
        let evidence = temp.appendingPathComponent("evidence", isDirectory: true)
        try FileManager.default.createDirectory(at: evidence, withIntermediateDirectories: true)
        let config = HvfEngineConfig(
            targetDiskPath: temp.appendingPathComponent("windows.raw").path,
            uefiVarsPath: temp.appendingPathComponent("vars.fd").path,
            evidenceDir: evidence.path,
            watchdogMs: nil, ramMiB: 6144, smpCpus: 4, clipboardSync: true,
            shareHostDir: nil, shareGuestDir: nil, virtioNet: true, virtioGpu3d: true,
            nvmeBufferedIO: true, ctlFilePath: temp.appendingPathComponent("hvf.ctl").path
        )
        let probe = AttachedLivenessProbe()
        let session = HvfEngineSession(config: config, repoRoot: temp) { _ in probe.lookUp() }
        XCTAssertTrue(session.attachToRunningVM())
        let dueLookup = { session.nextAttachedLivenessCheck = .distantPast; session.poll() }

        // The first due lookup blocks off the main thread; polls meanwhile start no second one.
        probe.answers = [false, true, false]
        dueLookup()
        guard await eventually({ probe.offMainLookups == 1 }) else {
            return XCTFail("poll() looked the attached VM up on the main thread")
        }
        for _ in 0..<3 { dueLookup() }
        try await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertEqual(probe.offMainLookups, 1)

        // The session stops and attaches again before the blocked lookup answers "gone": that answer is dropped.
        session.markStopped()
        XCTAssertTrue(session.attachToRunningVM())
        probe.release.signal()
        guard await eventually({ probe.offMainLookups == 2 || !session.attachedToExistingProcess }, step: dueLookup) else {
            return XCTFail("no lookup after the stale one")
        }
        XCTAssertTrue(session.attachedToExistingProcess, "the stale answer stopped the new attachment")
        XCTAssertNotEqual(session.connectionState, .stopped)

        // A current "gone" answer still stops the session.
        guard await eventually({ session.connectionState == .stopped }, step: dueLookup) else {
            return XCTFail("the session never noticed the VM was gone")
        }
        XCTAssertFalse(session.attachedToExistingProcess)
        XCTAssertEqual(probe.mainLookups, 2, "only the two attaches look the VM up on the main thread")
    }

    @MainActor
    private func eventually(_ condition: () -> Bool, step: () -> Void = {}) async -> Bool {
        let deadline = Date().addingTimeInterval(2)
        while !condition() {
            guard Date() < deadline else { return false }
            step()
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        return true
    }
}

/// Answers the attach's own main-thread lookups with "running"; off-main lookups take `answers` in order.
private final class AttachedLivenessProbe: @unchecked Sendable {
    private let lock = NSLock()
    private var queued: [Bool] = [], offMain = 0, onMain = 0
    let release = DispatchSemaphore(value: 0) // holds the first off-main lookup
    var answers: [Bool] {
        get { lock.withLock { queued } }
        set { lock.withLock { queued = newValue } }
    }
    var offMainLookups: Int { lock.withLock { offMain } }
    var mainLookups: Int { lock.withLock { onMain } }

    func lookUp() -> Bool {
        if Thread.isMainThread { lock.withLock { onMain += 1 }; return true }
        let (count, answer) = lock.withLock { () -> (Int, Bool) in
            offMain += 1
            return (offMain, queued.isEmpty ? true : queued.removeFirst())
        }
        if count == 1 { release.wait() }
        return answer
    }
}
