import Foundation

struct T19BitLockerTimeouts: Equatable {
    var boot: TimeInterval = 600, asset: TimeInterval = 60, launch: TimeInterval = 60, result: TimeInterval = 120
    var restart: TimeInterval = 180, shutdown: TimeInterval = 180, productStop: TimeInterval = 240
}

/// The opt-in T19 BitLocker restart check (docs/history/windows-hvf/bitlocker-restart-check-20261023.md):
/// an enable boot, a guest-initiated restart (in product mode the exit-42 respawn that power-cycles the
/// vTPM) and the unlock boot after it, then a stop. It never throws: every failure is a fixed reason
/// after a bounded wait. The journey's final run log is held aside and put back unchanged.
struct T19BitLockerCheck {
    static let assets = T19BitLockerGuestAssets.names + ["bv-product-e2e-tpm.cs"]
    let request: any T17JourneyRequest
    let appBundlePath: String
    let ui: T17UIControlling
    var fileManager: FileManager = .default
    var timeouts = T19BitLockerTimeouts()
    /// Whether no process names this disk path, the observation the app's own running check uses.
    var vmGone: (String) -> Bool = T19BitLockerProcesses.absent

    func run() -> T19BitLockerEvidence {
        var evidence = T19BitLockerEvidence()
        guard hold() else { evidence.reason = .logHoldFailed; return evidence }
        evidence.outcome = .inconclusive
        T19BitLockerGuestAssets.stage(appBundlePath: appBundlePath, sharePath: request.sharePath, fileManager: fileManager)
        let started = (try? ui.press("bridgevm.windows.runtime.start", timeout: 30)) != nil
        var up = false
        if started { up = sequence(&evidence) } else {
            evidence.enable = T19BitLockerBootRecord(ready: false, reason: .bootNotReady); evidence.reason = .bootNotReady
        }
        evidence.stop = started || !vmGone(request.diskPath) ? stop(guestUp: up) : .notStarted
        release()
        return evidence
    }

    /// Returns whether the agent should still answer a shutdown command.
    private func sequence(_ evidence: inout T19BitLockerEvidence) -> Bool {
        guard poll(timeouts.boot, { T19BitLockerRunLog.ready(runLog, after: 0) }) else {
            evidence.enable = T19BitLockerBootRecord(ready: false, reason: .bootNotReady); evidence.reason = .bootNotReady
            return false
        }
        evidence.enable.ready = true
        guard poll(timeouts.asset, { T19BitLockerRunLog.synchronized(Self.assets, in: runLog, after: 0) }) else {
            evidence.enable.reason = .assetMissing; evidence.reason = .assetMissing
            return true
        }
        evidence.enable.reason = nil
        switch read(.enable) {
        case .failure(let reason):
            evidence.enable.reason = reason; evidence.reason = reason
            return true
        case .success(let state):
            evidence.enable.state = state
            if let refusal = state.outcome { evidence.enable.reason = refusal; evidence.outcome = .refused; evidence.reason = refusal; return true }
        }
        let before = T17TPMRunLog.size(runLog)
        try? T17RuntimeControlInput.enter("shutdown.exe /r /t 0 /f", ui: ui)
        guard poll(timeouts.restart, { T19BitLockerRunLog.reset(runLog, after: before) != nil }),
              let reset = T19BitLockerRunLog.reset(runLog, after: before) else {
            evidence.restartObserved = false; evidence.reason = .restartNotObserved
            return true
        }
        evidence.restartObserved = true
        let ready = poll(timeouts.boot, { T19BitLockerRunLog.ready(runLog, after: reset) })
        evidence.powerCycleAfterRestart = T19BitLockerRunLog.powerCycled(runLog, after: reset)
        evidence.unlock.ready = ready
        guard ready else {
            evidence.unlock.reason = .unlockBootNotReady; evidence.reason = .unlockBootNotReady
            // Only a new helper generation that never reached READY suggests BitLocker recovery; a reset
            // that ended the run (the reboot limit) prints no power cycle and stays inconclusive.
            if evidence.powerCycleAfterRestart == true { evidence.outcome = .recoverySuspected }
            return false
        }
        guard poll(timeouts.asset, { T19BitLockerRunLog.synchronized(Self.assets, in: runLog, after: reset) }) else {
            evidence.unlock.reason = .assetMissing; evidence.reason = .assetMissing
            return true
        }
        switch read(.status) {
        case .failure(let reason):
            evidence.unlock.reason = reason; evidence.reason = reason
        case .success(let state):
            evidence.unlock.state = state
            let unexpected: T19BitLockerReason? = Self.unlockedUnderTPM(state) ? nil : .unlockStateUnexpected
            evidence.unlock.reason = state.outcome ?? unexpected; evidence.reason = evidence.unlock.reason
            if evidence.reason == nil { evidence.outcome = .unlocked }
        }
        return true
    }

    /// An unlocked volume under or after conversion whose only protector is a TPM protector.
    static func unlockedUnderTPM(_ state: T19BitLockerGuestState) -> Bool {
        state.outcome == nil && [1, 2, 4].contains(state.conversion ?? -1) && state.lock == 0 && state.protectors == [1]
    }

    /// Launches one phase through the BitLocker launcher and reads its result.
    private func read(_ phase: T19BitLockerPhase) -> Result<T19BitLockerGuestState, T19BitLockerReason> {
        let action = phase == .enable ? "BitLockerEnable" : "BitLockerStatus"
        let command = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\\bridgevm-share\\bv-product-e2e-bitlocker-launch.ps1 -Action \(action) -Nonce \(request.nonce)"
        let before = T17TPMRunLog.size(runLog)
        var outcome: Result<T19BitLockerGuestState, T19BitLockerReason> = .failure(.launchFailed)
        if (try? T17RuntimeControlInput.enter(command, ui: ui)) != nil, poll(timeouts.launch, {
            T17TPMRunLog.replied(runLog, after: before, command: command, marker: "T19-BITLOCKER-LAUNCHED-\(action)-\(prefix)")
        }) {
            let url = URL(fileURLWithPath: request.sharePath).appendingPathComponent(T19BitLockerGuestResult.name(phase: phase, prefix: prefix))
            outcome = .failure(.resultMissing)
            // A malformed result is read again until the deadline: the share may still be copying it.
            _ = poll(timeouts.result) {
                outcome = T19BitLockerGuestResult.read(url, nonce: request.nonce, phase: phase)
                return (try? outcome.get()) != nil
            }
        }
        return outcome
    }

    private func stop(guestUp: Bool) -> T19BitLockerStop {
        if guestUp {
            let before = T17TPMRunLog.size(runLog)
            if (try? T17RuntimeControlInput.enter("shutdown.exe /s /t 0 /f", ui: ui)) != nil,
               poll(timeouts.shutdown, { T19BitLockerRunLog.systemOff(runLog, after: before) }), stableLog() { return .guestShutdown }
        }
        try? ui.press("bridgevm.windows.runtime.stop", timeout: 10)
        return poll(timeouts.productStop, { vmGone(request.diskPath) }) ? .productStop : .unconfirmed
    }

    /// Moves the journey's final run log aside, so the check's boots get their own log.
    private func hold() -> Bool {
        guard T17TPMRunLog.regularSize(runLog) != nil, !fileManager.fileExists(atPath: held.path) else { return false }
        return (try? fileManager.moveItem(at: runLog, to: held)) != nil
    }

    /// Keeps the check's log beside the journey evidence and puts the journey's final log back unchanged.
    private func release() {
        if fileManager.fileExists(atPath: runLog.path), !fileManager.fileExists(atPath: archive.path) {
            try? fileManager.moveItem(at: runLog, to: archive)
        }
        if !fileManager.fileExists(atPath: runLog.path) { try? fileManager.moveItem(at: held, to: runLog) }
    }

    private func stableLog() -> Bool {
        var prior = -1, unchanged = 0
        return poll(30) {
            let size = T17TPMRunLog.regularSize(runLog) ?? -2
            unchanged = size == prior ? unchanged + 1 : 0
            prior = size
            return unchanged >= 5
        }
    }

    private func poll(_ timeout: TimeInterval, _ predicate: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if predicate() { return true }
            RunLoop.current.run(until: Date().addingTimeInterval(0.2))
        } while Date() < deadline
        return predicate()
    }

    private var prefix: String { String(request.nonce.prefix(12)) }
    private var bundle: URL { URL(fileURLWithPath: request.bundlePath, isDirectory: true) }
    private var runLog: URL { bundle.appendingPathComponent("logs/hvf/run.log") }
    private var held: URL { bundle.appendingPathComponent("metadata/product-e2e/bitlocker-held-run.log") }
    private var archive: URL { bundle.appendingPathComponent("metadata/product-e2e/bitlocker-run.log") }
}

/// `pgrep -f` on a literal path, as product_e2e_process_absent.py and the app's running check use it.
/// Only a completed observation with no match proves absence.
enum T19BitLockerProcesses {
    static func absent(_ path: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
        process.arguments = ["-f", path.reduce(into: "") { $0 += ".[]\\*^$()+?{}|".contains($1) ? "\\\($1)" : String($1) }]
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        guard (try? process.run()) != nil else { return false }
        let deadline = Date().addingTimeInterval(10)
        while process.isRunning, Date() < deadline { RunLoop.current.run(until: Date().addingTimeInterval(0.05)) }
        if process.isRunning { process.terminate(); return false }
        return process.terminationStatus == 1
    }
}
