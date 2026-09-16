import Foundation

extension HvfEngineSession {
    @discardableResult
    func start(policy: HvfRuntimeStartPolicy = .attachOrStart) -> HvfRuntimeStartOutcome {
        guard !runtimeTransitionInProgress, ownedStartOperation?.reservesSession != true, mutationReservation == nil else { return .refused("Runtime state transition in progress") }
        if let refusal = workAdmissionGate.check(workAdmission, reportRefusal: true) { return .refused(refusal) }
        guard !mayHaveOwnedWork, process?.isRunning != true else {
            return .refused("The owned runtime has not confirmed complete cleanup")
        }
        if process != nil { markStopped() }
        if policy == .requireNew, attachedToExistingProcess || processIsRunning(config.targetDiskPath) {
            return .refused("An existing runtime cannot be adopted by a require-new launch")
        }
        if policy == .attachOrStart, attachToRunningVM(reportDuplicateLaunch: true) { return .observedAttachment }
        runtimeTransitionInProgress = true
        defer { runtimeTransitionInProgress = false }
        let frozen = config
        if let failure = HvfRuntimeLaunchReadiness.failure(config: frozen, repoRoot: repoRoot,
            diagnostic: { append(.unknown($0)) }) {
            connectionState = .stopped
            return failure
        }
        timer?.invalidate(); timer = nil; process = nil; ownedController = nil
        closeLiveInput()
        let manifest: Data
        do { manifest = try HvfRuntimePreparation.prepare(config: frozen) }
        catch {
            connectionState = .stopped
            return .failed(.preparation, error.localizedDescription)
        }
        let runner = repoRoot.appendingPathComponent("target/release/hvf-runner")
        let typed = FileManager.default.isExecutableFile(atPath: runner.path)
        guard typed || FileManager.default.isExecutableFile(atPath:
            repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh").path) else {
            connectionState = .stopped
            return .failed(.helper, "The installed-boot wrapper is unavailable")
        }
        tailReader = TailOffsetReader(); lastHeartbeatDate = nil; lastHeartbeatAge = nil
        serviceStarted = false; stopCommandSent = false; stopDeadline = nil
        events = []; liveInputWriteFailureReported = false
        do {
            var deliveryFailure: String?
            if typed {
                let launched = try HvfOwnedRuntimeLaunch.start(config: frozen, repoRoot: repoRoot,
                    runner: runner, manifest: manifest, keyProvider: vtpmKeyProvider, launch: processLaunch)
                let controller = launched.controller
                ownedController = controller; process = controller.process
                ownedProcessIdentity = controller.identity
                let token = controller.identity.token
                controller.onChange = { [weak self] in self?.ownedRuntimeChanged(token: token) }
                controller.onDiagnostic = { [weak self] message in
                    guard self?.ownedController?.identity.token == token else { return }
                    self?.append(.unknown(message))
                }
                controller.activate(helloFrame: launched.helloFrame)
            } else {
                let launched = try HvfRuntimeLegacyLaunch.start(config: frozen, repoRoot: repoRoot,
                    keyProvider: vtpmKeyProvider, launch: processLaunch)
                process = launched.process; deliveryFailure = launched.keyDeliveryFailure
                ownedProcessIdentity = HvfOwnedRuntimeIdentity(token: UUID(), processID: launched.process.processIdentifier)
            }
            lastOwnedExit = nil; attachedToExistingProcess = false
            connectionState = deliveryFailure == nil ? .booting : .stopping
            guard let identity = ownedProcessIdentity else { return .failed(.processLaunch, "Missing retained runtime") }
            beginOwnedInputBoot(); startPolling()
            if let deliveryFailure { return .failed(.keyDelivery, deliveryFailure) }
            return .ownedLaunchAccepted(identity)
        } catch {
            connectionState = .stopped; process = nil
            append(.unknown("launch failed: \(error.localizedDescription)"))
            if let failure = error as? HvfRuntimeLaunchFailure { return .failed(failure.stage, failure.detail) }
            return .failed(.processLaunch, error.localizedDescription)
        }
    }

}
