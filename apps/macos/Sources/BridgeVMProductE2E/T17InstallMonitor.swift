import Foundation

enum T17InstallMonitor {
    static let stageIdentifier = "bridgevm.windows.install.stage"
    static let failureIdentifier = "bridgevm.windows.install.failure"

    static func wait(
        timeout: TimeInterval = 1_800,
        clock: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: (TimeInterval) -> Void = { RunLoop.current.run(until: Date().addingTimeInterval($0)) },
        applicationIsRunning: () -> Bool,
        stage: () -> String?,
        failure: () -> String?
    ) throws {
        let deadline = clock() + timeout
        repeat {
            guard applicationIsRunning() else {
                throw T17Blocker(code: "installer-failed", detail: "product app exited during installation")
            }
            switch stage() {
            case "완료": return
            case "실패":
                let observed = failure()?.trimmingCharacters(in: .whitespacesAndNewlines)
                let message = observed.flatMap { $0.isEmpty ? nil : $0 } ?? "product install reported a failed UI stage"
                throw T17Blocker(code: "installer-failed", detail: String(message.prefix(512)))
            default: pause(0.2)
            }
        } while clock() < deadline
        throw T17Blocker(code: "installer-failed", detail: "product install did not reach a terminal UI stage")
    }
}
