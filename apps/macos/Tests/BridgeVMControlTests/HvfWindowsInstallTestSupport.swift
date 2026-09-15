import Foundation
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPipelineQueue {
    var jobs: [@MainActor () async -> Void] = []
    func enqueue(_ work: @escaping @MainActor () async -> Void) { jobs.append(work) }
    func runNext() async { await jobs.removeFirst()() }
    func discard() { jobs.removeAll() }
}

// The lock protects all mutable observations; both semaphores are thread-safe.
final class HvfWindowsInstallValidationProbe: @unchecked Sendable {
    struct Snapshot: Sendable {
        let threadMain: [Bool]
        let finished: Int
        let gateTimedOut: Bool
        var calls: Int { threadMain.count }
    }

    private let entry = DispatchSemaphore(value: 0)
    private let lock = NSLock()
    private let gate = DispatchSemaphore(value: 0)
    private let gateFirstCall: Bool
    private var error: String?
    private var threadMain: [Bool] = []
    private var finished = 0
    private var gateTimedOut = false

    init(error: String? = nil, gateFirstCall: Bool = false) {
        self.error = error
        self.gateFirstCall = gateFirstCall
    }

    func setError(_ error: String?) {
        lock.lock(); self.error = error; lock.unlock()
    }

    func validate(_ plan: HvfWindowsInstallPlan) -> String? {
        let onMain = Thread.isMainThread
        lock.lock()
        threadMain.append(onMain)
        let first = threadMain.count == 1
        let result = error
        lock.unlock()
        if first { entry.signal() }
        // A main-thread regression fails observations without blocking the test's main actor.
        let timedOut = first && gateFirstCall && !onMain
            && gate.wait(timeout: .now() + 10) == .timedOut
        lock.lock()
        finished += 1
        gateTimedOut = gateTimedOut || timedOut
        lock.unlock()
        return result
    }

    func waitForEntry() async -> Bool {
        await Task.detached { [entry] in
            entry.wait(timeout: .now() + 5) == .success
        }.value
    }

    func release() { gate.signal() }

    var snapshot: Snapshot {
        lock.lock()
        defer { lock.unlock() }
        return Snapshot(threadMain: threadMain, finished: finished, gateTimedOut: gateTimedOut)
    }
}

enum HvfWindowsInstallTestSupport {
    static func plan() -> HvfWindowsInstallPlan {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("install-validation-" + UUID().uuidString)
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        return HvfWindowsInstallPlan(repoRoot: root, libraryRoot: root.appendingPathComponent("library"),
            bundlePath: root.appendingPathComponent("bundle").path,
            slug: "validation-" + UUID().uuidString, request: request)
    }
}
