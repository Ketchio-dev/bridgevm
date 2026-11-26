import Foundation

enum T17InstallMonitor {
    static let stageIdentifier = "bridgevm.windows.install.stage"
    static let failureIdentifier = "bridgevm.windows.install.failure"

    static func wait(
        timeout: TimeInterval = 1_800,
        clock: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: (TimeInterval) -> Void = { RunLoop.current.run(until: Date().addingTimeInterval($0)) },
        applicationIsRunning: () -> Bool, installedRuntimeVisible: () -> Bool,
        stage: () -> String?,
        failure: () -> String?
    ) throws {
        let deadline = clock() + timeout
        repeat {
            guard applicationIsRunning() else {
                throw T17Blocker(code: "installer-failed", detail: "product app exited during installation")
            }
            switch stage().map(phaseToken) {
            case "done": return
            case "failed":
                let observed = failure()?.trimmingCharacters(in: .whitespacesAndNewlines)
                let message = observed.flatMap { $0.isEmpty ? nil : $0 } ?? "product install reported a failed UI stage"
                throw T17Blocker(code: "installer-failed", detail: String(message.prefix(512)))
            default: if installedRuntimeVisible() { return }; pause(0.2)
            }
        } while clock() < deadline
        throw T17Blocker(code: "installer-failed", detail: "product install did not reach a terminal UI stage")
    }

    // Earlier app builds exposed the same phases in Korean.
    static func phaseToken(_ value: String) -> String {
        switch value {
        case "Ready to install", "대기": return "idle"
        case "Checking installation files", "설치 입력 확인": return "validating"
        case "Preparing installation media", "설치 소스 준비": return "preparing-source"
        case "Installing Windows", "Windows 무인 설치": return "installing"
        case "Saving the VM", "VM에 반영": return "finalizing"
        case "Recovering the installation", "기존 설치 결과 복구": return "recovering"
        case "Cancelling…", "취소 중…": return "cancelling"
        case "Cancelled", "취소됨": return "cancelled"
        case "Complete", "완료": return "done"
        case "Failed", "실패": return "failed"
        default: return "unknown"
        }
    }
}
