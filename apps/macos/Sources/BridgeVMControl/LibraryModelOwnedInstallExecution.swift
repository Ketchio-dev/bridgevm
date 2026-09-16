import Foundation

extension LibraryModel {
    func beginOwnedInstallPreparation(_ request: NativeInstallControlRequest, saved: VMConfig,
        library: NativeRuntimeLibraryHandle, operation: NativeInstallOperation,
        validateOwner: @escaping () throws -> Void
    ) {
        let options = windowsInstallSessions.preparation.options
        guard let input = currentWindowsInstallPreparationInput(slug: saved.slug,
            repoRoot: options.repoRoot), input.config == saved else {
            operation.fail(HvfWindowsInstallPreparation.staleMessage); return
        }
        let expected = windowsInstallSessions.record(for: saved.slug)?.session
        let task = Task { [weak self, weak operation] in
            let plan = await HvfWindowsInstallPlanWorker.build(input, using: options.builder)
            guard !Task.isCancelled, let self, let operation else { return }
            finishOwnedInstallPreparation(plan, input: input, expected: expected, request: request,
                saved: saved, library: library, operation: operation, validateOwner: validateOwner)
        }
        operation.attach(task)
    }

    private func finishOwnedInstallPreparation(_ plan: HvfWindowsInstallPlan,
        input: HvfWindowsInstallPlanInput, expected: HvfWindowsInstallSession?,
        request: NativeInstallControlRequest, saved: VMConfig, library: NativeRuntimeLibraryHandle,
        operation: NativeInstallOperation, validateOwner: () throws -> Void
    ) {
        guard case let .success(current) = windowsInstallSessions.cliOperations.operation(
            vmID: saved.slug, digest: request.expectedSavedConfigurationDigest), current === operation,
            operation.reservesWork else { return }
        do {
            try validateOwner()
            guard try validatedInstallConfiguration(request, library: library) == saved else {
                throw NativeInstallControlRefusal.configurationChanged
            }
        } catch {
            operation.fail(HvfWindowsInstallPreparation.staleMessage); return
        }
        switch windowsInstallPreparationResult(plan, input: input, replacing: expected) {
        case let .failed(message): operation.fail(message)
        case .preparing: operation.fail(HvfWindowsInstallPreparation.staleMessage)
        case let .ready(session):
            bindOwnedInstallSession(session, saved: saved, operation: operation)
            guard session.start() != nil else {
                operation.fail("Windows 설치를 시작할 수 없습니다. VM 상태를 확인한 뒤 다시 시도하세요."); return
            }
            operation.adopt(session)
        }
    }

    private func bindOwnedInstallSession(_ session: HvfWindowsInstallSession, saved: VMConfig,
                                         operation: NativeInstallOperation) {
        session.workAdmission = { [weak self, weak session, weak operation] report in
            guard let self, let session, let operation else {
                return "VM 라이브러리를 사용할 수 없습니다. 라이브러리 화면에서 다시 시도하세요."
            }
            let refusal = self.libraryWorkRefusal(slug: saved.slug, excludingInstall: operation,
                owns: { owner, current in current == saved && owner.windowsInstallSessions.owns(session, for: current) })
            if report, let refusal { self.operationError = refusal }
            return refusal
        }
        session.onCompleted = { [weak self] in self?.reload() }
    }
}
