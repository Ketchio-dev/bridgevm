import Foundation
import Combine
import XCTest
@testable import BridgeVMControl

final class HvfRuntimeWorkAdmissionBackend: VMBackend {
    enum Operation: CaseIterable { case start, resources }
    struct Calls: Equatable {
        var starts = 0, writes = 0, capability = 0, stops = 0, liveness = 0
        var ip = 0, resources = 0, guest = 0
    }
    private let lock = NSLock()
    private let entry = DispatchSemaphore(value: 0)
    private let release = DispatchSemaphore(value: 0)
    private var held: Operation?
    private var recorded = Calls()
    private var timedOut = false
    let startSucceeds: Bool
    let displayName = "owned fixture", kind = "fixture"
    let supportsGuestCommands = false, supportsPackageInstall = false
    let supportsClipboard = false, supportsSSH = false

    init(startSucceeds: Bool = false) { self.startSucceeds = startSucceeds }
    var calls: Calls { lock.lock(); defer { lock.unlock() }; return recorded }
    var gateTimedOut: Bool { lock.lock(); defer { lock.unlock() }; return timedOut }
    var supportsResourceChanges: Bool { record(\.capability); return true }
    func hold(_ operation: Operation) { lock.lock(); held = operation; lock.unlock() }
    func releaseGate() { release.signal() }
    func waitForEntry() async -> Bool {
        await Task.detached { [entry] in entry.wait(timeout: .now() + 5) == .success }.value
    }
    private func record(_ key: WritableKeyPath<Calls, Int>) {
        lock.lock(); recorded[keyPath: key] += 1; lock.unlock()
    }
    private func enter(_ operation: Operation) {
        lock.lock(); let shouldWait = held == operation; lock.unlock()
        entry.signal()
        if shouldWait && release.wait(timeout: .now() + 10) == .timedOut {
            lock.lock(); timedOut = true; lock.unlock()
        }
    }
    func start() -> Bool { record(\.starts); enter(.start); return startSucceeds }
    func setResources(memMiB: Int, cpu: Int) -> Bool {
        record(\.writes); enter(.resources); return false // Never reaches restart or file writes.
    }
    func stop() { record(\.stops) }
    func isRunning() -> Bool { record(\.liveness); return false }
    func currentIP() -> String? { record(\.ip); return nil }
    func resources() -> (memMiB: Int, cpu: Int) { record(\.resources); return (4096, 2) }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        record(\.guest); return ("", -1)
    }
}

@MainActor
final class HvfRuntimeWorkAdmissionOperation {
    private let model: ControlModel
    private let operation: HvfRuntimeWorkAdmissionBackend.Operation
    private let completed = DispatchSemaphore(value: 0)
    private var subscription: AnyCancellable?

    init(_ operation: HvfRuntimeWorkAdmissionBackend.Operation, model: ControlModel) {
        self.operation = operation
        self.model = model
        let succeeds = (model.backend as? HvfRuntimeWorkAdmissionBackend)?.startSucceeds == true
        let terminal = operation == .resources
            ? "리소스 적용 실패 — VM은 재시작하지 않았습니다."
            : succeeds ? "VM 부팅 중…" : "VM 시작 실패"
        subscription = model.$statusNote.dropFirst().sink { [completed] note in
            if note == terminal { completed.signal() }
        }
    }

    func invoke() -> Bool {
        guard !model.lifecycleBusy else {
            XCTFail("Operation tracker requires an initially idle lifecycle"); return false
        }
        guard model.backend is HvfRuntimeWorkAdmissionBackend else {
            XCTFail("Never invoke a real backend in an admission fixture"); return false
        }
        switch operation {
        case .start: model.start()
        case .resources: model.applyResources()
        }
        return model.lifecycleBusy
    }

    func acknowledge(if accepted: Bool) async -> Bool {
        guard accepted else { subscription?.cancel(); return true }
        let acknowledged = await Task.detached { [completed] in
            completed.wait(timeout: .now() + 5) == .success
        }.value
        subscription?.cancel()
        XCTAssertTrue(acknowledged, "The accepted fake operation must publish its MainActor completion")
        XCTAssertFalse(model.lifecycleBusy)
        return acknowledged && !model.lifecycleBusy
    }
}

@MainActor
struct HvfRuntimeWorkAdmissionControlState: Equatable {
    let running: Bool, busy: Bool, lifecycleBusy: Bool
    let status: String, ip: String
    let memory: Double, cpu: Int, pendingMemory: Double, pendingCPU: Double
    init(_ model: ControlModel) {
        running = model.running; busy = model.busy; lifecycleBusy = model.lifecycleBusy
        status = model.statusNote; ip = model.ip
        memory = model.memGiB; cpu = model.cpu
        pendingMemory = model.pendingMemGiB; pendingCPU = model.pendingCPU
    }
}
