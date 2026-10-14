import Foundation

/// An in-memory launch group. Paths never grant ownership of a process.
final class GenericOwnedRuntime: @unchecked Sendable {
    enum Phase { case starting, running, stopping, terminal }
    struct Snapshot {
        let phase: Phase
        let primaryRunning: Bool
        let failure: String?
        var isActive: Bool { phase != .terminal }
    }
    let identity = UUID()
    private let lock = NSLock()
    private let queue = DispatchQueue(label: "BridgeVM.generic-owned-launch")
    private let wake = DispatchSemaphore(value: 0)
    private var state = Snapshot(phase: .starting, primaryRunning: false, failure: nil)
    private var stopRequested = false
    // Only the serial queue touches these handles. No cleanup occurs on deinit.
    private var primary: Process?
    private var dependency: Process?
    private let spawn: (GenericOwnedProcessSpec) throws -> Process
    private let isDependencyRunning: (String) -> Bool

    init(plan: GenericOwnedLaunchPlan,
         spawn: @escaping (GenericOwnedProcessSpec) throws -> Process = GenericOwnedProcessLauncher.launch,
         isDependencyRunning: @escaping (String) -> Bool = Shell.isProcessRunning(matching:)) {
        self.spawn = spawn
        self.isDependencyRunning = isDependencyRunning
        queue.async { self.begin(plan) }
    }

    var snapshot: Snapshot { lock.withLock { state } }

    func requestStop() {
        lock.withLock { stopRequested = true }
        wake.signal()
        queue.async { self.stopChildren() }
    }

    private var cancelled: Bool { lock.withLock { stopRequested } }

    private func publish(_ phase: Phase, failure: String? = nil) {
        let running = primary?.isRunning == true
        lock.withLock {
            state = Snapshot(phase: phase, primaryRunning: running, failure: failure ?? state.failure)
        }
    }

    private func begin(_ plan: GenericOwnedLaunchPlan) {
        do {
            guard !cancelled else { stopChildren(); return }
            for path in plan.directories {
                try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
            }
            if let tpm = plan.tpm {
                guard !cancelled else { stopChildren(); return }
                // Reuse preserves compatibility but never creates an owned handle.
                if !isDependencyRunning(tpm.reusePattern) {
                    guard let child = try spawnUnlessCancelled(tpm.process) else { stopChildren(); return }
                    dependency = child
                    observe(child)
                }
                if tpm.launchDelay > 0 {
                    _ = wake.wait(timeout: .now() + tpm.launchDelay)
                }
                guard !cancelled else { stopChildren(); return }
                if let dependency, !dependency.isRunning {
                    publish(.terminal, failure: "TPM 프로세스가 시작 중 종료되었습니다.")
                    return
                }
            }
            guard !cancelled else { stopChildren(); return }
            guard let child = try spawnUnlessCancelled(plan.primary) else { stopChildren(); return }
            primary = child
            observe(child)
            publish(.running)
            if cancelled { stopChildren() }
        } catch {
            publish(.stopping, failure: "VM 프로세스를 실행하지 못했습니다.")
            stopChildren()
        }
    }

    private func spawnUnlessCancelled(_ spec: GenericOwnedProcessSpec) throws -> Process? {
        // Stop linearizes against creation: a stop accepted first prevents spawn.
        try lock.withLock {
            guard !stopRequested else { return nil }
            return try spawn(spec)
        }
    }

    private func observe(_ process: Process) {
        process.terminationHandler = { [weak self] _ in
            guard let self else { return }
            self.queue.async { self.childExited() }
        }
        // A short-lived child can exit before its handler is installed.
        if !process.isRunning { queue.async { self.childExited() } }
    }

    private func childExited() {
        if primary?.isRunning == true {
            publish(cancelled ? .stopping : .running)
            return
        }
        // The primary has exited: only our own auxiliary may be cleaned up.
        stopChildren()
    }

    private func stopChildren() {
        if let primary, primary.isRunning {
            publish(.stopping)
            primary.terminate()
        } else if let dependency, dependency.isRunning {
            publish(.stopping)
            dependency.terminate()
        } else {
            publish(.terminal)
        }
    }
}
