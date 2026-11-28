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
            return "Waiting for Guest Tools to accept the shutdown request. To stop waiting, use Force Stop."
        case .windowsClosingApps:
            return "Windows is closing apps. Save any open work in the VM window, or use Force Stop to shut down now."
        case .windowsStillRunning:
            return "Windows is still running. Check the VM window for an app blocking shutdown, then choose Shut Down Again, or use Force Stop."
        }
    }
}
