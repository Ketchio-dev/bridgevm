import Foundation

enum T17OwnedApplicationCleanup {
    static func stop(
        installationPending: Bool,
        press: (String, TimeInterval) throws -> Void,
        installStage: () throws -> String?,
        systemOffObserved: () -> Bool,
        applicationIsRunning: () -> Bool,
        terminate: () -> Void,
        interrupt: () -> Void,
        clock: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: (TimeInterval) -> Void = { RunLoop.current.run(until: Date().addingTimeInterval($0)) }
    ) -> Bool {
        func wait(_ seconds: TimeInterval, _ predicate: () -> Bool) -> Bool {
            let deadline = clock() + seconds
            repeat {
                if predicate() { return true }
                pause(0.2)
            } while clock() < deadline
            return predicate()
        }
        var installationStopped = true
        if installationPending {
            try? press("bridgevm.windows.install.cancel", 2)
            // Active installer work returns before its session publishes a terminal stage.
            // A missing/failed query cannot be replaced by the app's later exit.
            installationStopped = wait(30) {
                guard let stage = try? installStage() else { return false }
                return ["취소됨", "실패", "완료"].contains(stage)
            }
        } else {
            try? press("bridgevm.windows.runtime.stop", 2)
            _ = wait(30, systemOffObserved)
        }
        if applicationIsRunning() { terminate() }
        _ = wait(10) { !applicationIsRunning() }
        if applicationIsRunning() { interrupt() }
        _ = wait(5) { !applicationIsRunning() }
        return installationStopped && !applicationIsRunning()
    }
}
