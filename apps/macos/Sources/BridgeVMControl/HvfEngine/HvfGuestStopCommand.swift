import Foundation

/// The guest command a stop sends to Windows.
enum HvfGuestStopCommand: Equatable, Sendable {
    /// The user's Stop and quitting the app: Windows asks each app to close, so the user can still
    /// save, and an app with unsaved work holds the shutdown on Windows' own screen.
    case shutdown
    /// An explicit Force Stop: Windows closes apps without asking and turns off at once.
    case forceShutdown

    var line: String {
        switch self {
        case .shutdown: return "shutdown.exe /s /t 0"
        case .forceShutdown: return "shutdown.exe /p /f"
        }
    }
}

/// What Windows is doing while a Stop waits, for the stop controls.
enum HvfGuestStopWait: Equatable, Sendable {
    case waitingForGuestAgent, windowsClosingApps, windowsStillRunning

    var message: String {
        switch self {
        case .waitingForGuestAgent:
            return "Windows 에이전트가 준비되면 종료를 요청합니다. 기다리지 않으려면 강제 중단하세요."
        case .windowsClosingApps:
            return "Windows가 앱을 닫고 종료하는 중입니다. 저장하지 않은 작업이 있으면 VM 화면에서 저장하거나 닫으세요. 기다리지 않으려면 강제 중단하세요."
        case .windowsStillRunning:
            return "Windows가 아직 종료되지 않았습니다. 앱이 종료를 막고 있을 수 있으니 VM 화면을 확인하거나 강제 중단하세요."
        }
    }
}
