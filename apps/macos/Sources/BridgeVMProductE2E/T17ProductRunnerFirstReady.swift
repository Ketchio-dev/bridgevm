import Foundation

extension T17ProductRunner {
    func bootToFirstReady(_ ui: T17UIControlling) throws -> String {
        if (try? ui.waitFor("bridgevm.windows.runtime.view", timeout: 1)) == nil { try ui.press("bridgevm.dashboard.advanced", timeout: 60) }
        try ui.waitFor("bridgevm.windows.runtime.view", timeout: 60)
        try T17RuntimeIntegrationSetup.apply(sharePath: request.sharePath, ui: ui)
        try ui.press("bridgevm.windows.runtime.start", timeout: 20)
        let log = bundle.appendingPathComponent("logs/hvf/run.log")
        return try T17FirstReadyWaiter.wait(observe: {
            let ready = T17BoundedLog.lines(log).first {
                $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT PONG (proactive)")
            }
            return try T17FirstReadyObservation.capture(
                readyLine: ready, applicationRunning: self.application?.isRunning == true, ui: ui)
        }, diagnostic: { T17FirstBootDiagnostic.capture(log) }, timeoutDiagnostic: {
            T17FirstReadyStopCapture.capture(log: log, laneRoot: URL(fileURLWithPath: self.request.laneRoot),
                applicationRunning: { self.application?.isRunning == true },
                ownedRuntimeState: { try? ui.text("bridgevm.windows.runtime.state", timeout: 1) })
                + "; " + T17FirstBootDiagnostic.capture(log)
        })
    }

    /// A journey failure after first READY freezes the guest with a host diagnostic
    /// stop, so the private packet keeps the screen as it was at failure instead of
    /// the black frame a guest shutdown leaves.
    func freezeAfterFirstReady() -> String {
        T17FirstReadyStopCapture.capture(log: bundle.appendingPathComponent("logs/hvf/run.log"),
            laneRoot: URL(fileURLWithPath: request.laneRoot), applicationRunning: { self.application?.isRunning == true },
            ownedRuntimeState: { try? self.ui?.text("bridgevm.windows.runtime.state", timeout: 1) })
    }
}
