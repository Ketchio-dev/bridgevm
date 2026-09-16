import Foundation
@testable import BridgeVMControl

// Only synthetic policy fixtures use this finite gate. All mutable observations are locked.
final class HvfWindowsInstallRecoveryProbe: @unchecked Sendable {
    enum Gate { case inspection, seeding }
    struct Snapshot {
        let inspections: Int, recoveries: Int, seeds: Int
        let workerThreads: [Bool], gateTimedOut: Bool
    }
    private let lock = NSLock()
    private let entered = DispatchSemaphore(value: 0), released = DispatchSemaphore(value: 0)
    private var gate: Gate?
    private var failSeeder = false
    private var inspections = 0, recoveries = 0, seeds = 0
    private var workerThreads: [Bool] = []
    private var gateTimedOut = false

    func arm(_ gate: Gate, failSeeder: Bool = false) {
        lock.lock(); self.gate = gate; self.failSeeder = failSeeder; lock.unlock()
    }
    func inspect(_ plan: HvfWindowsInstallPlan) throws -> HvfWindowsInstallRecovery.Decision {
        lock.lock(); inspections += 1; workerThreads.append(Thread.isMainThread); lock.unlock()
        hold(.inspection)
        return try HvfWindowsInstallRecovery.inspect(plan: plan)
    }
    func recover(_ plan: HvfWindowsInstallPlan, ticket: HvfWindowsInstallRecovery.Ticket) throws -> VMConfig {
        lock.lock(); recoveries += 1; workerThreads.append(Thread.isMainThread); lock.unlock()
        return try HvfWindowsInstallRecovery.recover(plan: plan, ticket: ticket, secureBootSeeder: { vars, disk in
            self.lock.lock(); self.seeds += 1; let fail = self.failSeeder; self.lock.unlock()
            self.hold(.seeding)
            if fail { throw HvfWindowsInstallFinalizationError.invalidState("synthetic recovery seed failure") }
            return try HvfWindowsInstallRecoveryFixture.syntheticSeeder(vars, disk)
        })
    }
    private func hold(_ point: Gate) {
        lock.lock(); let shouldHold = gate == point; if shouldHold { gate = nil }; lock.unlock()
        guard shouldHold else { return }
        entered.signal()
        // A main-thread regression is observable without deadlocking the test actor.
        let timedOut = !Thread.isMainThread && released.wait(timeout: .now() + 5) == .timedOut
        lock.lock(); gateTimedOut = gateTimedOut || timedOut; lock.unlock()
    }
    func waitForEntry() async -> Bool {
        await Task.detached { [entered] in entered.wait(timeout: .now() + 5) == .success }.value
    }
    func release() { released.signal() }
    var snapshot: Snapshot {
        lock.lock(); defer { lock.unlock() }
        return .init(inspections: inspections, recoveries: recoveries, seeds: seeds,
            workerThreads: workerThreads, gateTimedOut: gateTimedOut)
    }
}
