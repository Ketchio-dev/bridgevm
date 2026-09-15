import Foundation
@testable import BridgeVMControl

/// Only this lock-protected observation/gate object crosses the worker boundary.
final class HvfWindowsInstallPlanPreparationProbe: @unchecked Sendable {
    struct Snapshot {
        let inputs: [HvfWindowsInstallPlanInput]
        let threadMain: [Bool]
        let finished: Int
        let invalidInputs: Int
        let gateTimedOut: Bool
        var calls: Int { inputs.count }
    }

    private let lock = NSLock()
    private let entries = (0..<16).map { _ in DispatchSemaphore(value: 0) }
    private let gates = (0..<16).map { _ in DispatchSemaphore(value: 0) }
    private let gatedCalls: Set<Int>
    private let safePlan: HvfWindowsInstallPlan
    private var inputs: [HvfWindowsInstallPlanInput] = []
    private var threadMain: [Bool] = []
    private var finished = 0, invalidInputs = 0
    private var gateTimedOut = false

    init(safePlan: HvfWindowsInstallPlan, gatedCalls: Set<Int> = []) {
        self.safePlan = safePlan
        self.gatedCalls = gatedCalls
    }

    func build(_ input: HvfWindowsInstallPlanInput) -> HvfWindowsInstallPlan {
        let onMain = Thread.isMainThread
        lock.lock()
        inputs.append(input)
        threadMain.append(onMain)
        let call = inputs.count
        let valid = input.repoRoot == safePlan.repoRoot && input.libraryRoot == safePlan.libraryRoot
            && input.request.isoPath == safePlan.request.isoPath
            && input.config.bundlePath.hasPrefix(safePlan.libraryRoot.path + "/")
        if !valid { invalidInputs += 1 }
        lock.unlock()
        if call <= entries.count { entries[call - 1].signal() }
        // An unexpected main-thread regression is observed without deadlocking the actor.
        let timedOut = gatedCalls.contains(call) && !onMain && call <= gates.count
            && gates[call - 1].wait(timeout: .now() + 10) == .timedOut
        // Wrong roots are a failed prerequisite, never permission to read fallback helpers.
        let result = valid ? input.makePlan() : safePlan
        lock.lock()
        finished += 1
        gateTimedOut = gateTimedOut || timedOut
        lock.unlock()
        return result
    }

    func waitForEntry(_ call: Int) async -> Bool {
        guard (1...entries.count).contains(call) else { return false }
        let entry = entries[call - 1]
        return await Task.detached { entry.wait(timeout: .now() + 5) == .success }.value
    }

    func release(_ call: Int) {
        if (1...gates.count).contains(call) { gates[call - 1].signal() }
    }

    func releaseAll() { gates.forEach { $0.signal() } }

    var snapshot: Snapshot {
        lock.lock()
        defer { lock.unlock() }
        return Snapshot(inputs: inputs, threadMain: threadMain, finished: finished,
            invalidInputs: invalidInputs, gateTimedOut: gateTimedOut)
    }
}
