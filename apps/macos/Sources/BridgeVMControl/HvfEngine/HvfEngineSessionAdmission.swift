extension HvfEngineSession {
    @discardableResult
    func acceptStartConfiguration(_ config: HvfEngineConfig) -> Bool {
        guard connectionState == .stopped else { return false }
        guard workAdmission?(true) == nil else { return false }
        self.config = config
        return true
    }

    @discardableResult
    func attachIfStopped() -> Bool {
        guard connectionState == .stopped else { return false }
        return attachToRunningVM(reportRefusal: false)
    }

}
