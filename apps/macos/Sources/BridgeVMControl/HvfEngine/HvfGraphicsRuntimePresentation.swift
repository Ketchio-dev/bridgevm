import Foundation

enum HvfGraphicsRuntimePresentation: Equatable {
    case nextLaunchBasic
    case nextLaunchExperimental
    case runningBasic
    case runningExperimental
    case runningUnverified

    static func classify(running: Bool, ownedLaunchRunning: Bool,
                         experimental3DAllowed: Bool?) -> Self {
        guard running else {
            return experimental3DAllowed == true ? .nextLaunchExperimental : .nextLaunchBasic
        }
        guard ownedLaunchRunning else { return .runningUnverified }
        return experimental3DAllowed == true ? .runningExperimental : .runningBasic
    }

    var label: String {
        switch self {
        case .nextLaunchBasic: return "Next start · Basic display · 3D off"
        case .nextLaunchExperimental: return "Next start · Graphics Lab · Experimental 3D"
        case .runningBasic: return "Current session · Basic display · 3D off"
        case .runningExperimental: return "Current session · Graphics Lab · Experimental 3D"
        case .runningUnverified: return "Current session · Graphics mode unknown"
        }
    }
}

extension HvfWindowsBackend {
    func graphicsPresentation(running: Bool) -> HvfGraphicsRuntimePresentation {
        .classify(running: running, ownedLaunchRunning: launchedProcess?.isRunning == true,
                  experimental3DAllowed: config.experimental3DAllowed)
    }
}

extension ControlModel {
    var graphicsPresentation: HvfGraphicsRuntimePresentation? {
        (backend as? HvfWindowsBackend)?.graphicsPresentation(running: running)
    }
}
