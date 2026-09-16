extension HvfEngineSession {
    @discardableResult
    func acceptStartConfiguration(_ config: HvfEngineConfig) -> Bool {
        guard !hasActiveRuntimeWork, !workAdmissionGate.isChecking else { return false }
        guard workAdmissionGate.check(workAdmission, reportRefusal: true) == nil else { return false }
        self.config = config
        return true
    }

    @discardableResult
    func attachIfStopped() -> Bool {
        guard !hasActiveRuntimeWork, !workAdmissionGate.isChecking else { return false }
        return attachToRunningVM(reportRefusal: false)
    }

}
