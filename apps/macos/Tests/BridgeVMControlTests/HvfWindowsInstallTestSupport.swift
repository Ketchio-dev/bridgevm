import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfWindowsInstallPipelineQueue {
    var jobs: [@MainActor () async -> Void] = []
    func enqueue(_ work: @escaping @MainActor () async -> Void) { jobs.append(work) }
    func runNext() async { await jobs.removeFirst()() }
    func discard() { jobs.removeAll() }
}

// The lock protects all mutable observations; the semaphore and expectation are thread-safe.
final class HvfWindowsInstallValidationProbe: @unchecked Sendable {
    struct Snapshot: Sendable {
        let threadMain: [Bool]
        let finished: Int
        let gateTimedOut: Bool
        var calls: Int { threadMain.count }
    }

    let entered = XCTestExpectation(description: "injected validation entered")
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
        if first { entered.fulfill() }
        // A main-thread regression fails observations without blocking the test's main actor.
        let timedOut = first && gateFirstCall && !onMain
            && gate.wait(timeout: .now() + 10) == .timedOut
        lock.lock()
        finished += 1
        gateTimedOut = gateTimedOut || timedOut
        lock.unlock()
        return result
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

@MainActor
final class HvfWindowsInstallStoreFixture {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("install-store-" + UUID().uuidString)
    let validator = HvfWindowsInstallValidationProbe()
    var jobs: [@MainActor () async -> Void] = []
    var made = 0

    func library() -> LibraryModel {
        LibraryModel(rootURL: root, migrateLegacy: false, installSessionFactory: { plan in
            self.made += 1
            return HvfWindowsInstallSession(plan: plan, validate: { [probe = self.validator] in
                probe.validate($0)
            }, schedule: { self.jobs.append($0) })
        })
    }

    func save(_ config: VMConfig, diskGiB: Int = 64) throws {
        let request = HvfWindowsInstallRequest(isoPath: root.appendingPathComponent("absent.iso").path,
            isoSHA256: String(repeating: "a", count: 64), diskGiB: diskGiB, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: config.bundlePath))
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
    }

    func config(_ slug: String = "windows") -> VMConfig {
        VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: root.appendingPathComponent(slug + "/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
            leasesPath: "", guestName: slug, displayWidth: 1280, displayHeight: 720, installPending: true)
    }

    func removeRegistration(_ config: VMConfig) throws {
        try FileManager.default.removeItem(at: root.appendingPathComponent(config.slug + "/vm.json"))
    }

    func finishCancelled(_ session: HvfWindowsInstallSession) async {
        session.cancel()
        let job = jobs.removeFirst()
        await job() // Only the existing pre-dispatch cancellation guard is exercised.
        XCTAssertFalse(session.isRunning)
    }

    func clean() {
        jobs.removeAll() // Queued pipeline work never executes during teardown.
        try? FileManager.default.removeItem(at: root)
    }
}
