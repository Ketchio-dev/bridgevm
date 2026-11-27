import Foundation

enum HvfGraphicsRuntimePresentation: Equatable {
    case nextLaunchBasic
    case nextLaunchExperimental
    case runningBasic
    case runningExperimental
    case runningUnverified

    /// `ownedRun3D` is the graphics of the run this app started, nil when the running Windows is
    /// another process's (attached, external): its mode is then unknown, whatever is saved.
    static func classify(running: Bool, ownedRun3D: Bool?, saved3D: Bool?) -> Self {
        guard running else {
            return saved3D == true ? .nextLaunchExperimental : .nextLaunchBasic
        }
        guard let ownedRun3D else { return .runningUnverified }
        return ownedRun3D ? .runningExperimental : .runningBasic
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

extension HvfEngineSession {
    /// Whether the Windows this app started, and that is still running, has experimental 3D.
    var ownedRunExperimental3D: Bool? {
        guard let controller = ownedController, controller.process.isRunning else { return nil }
        return controller.config.virtioGpu3d
    }
}
