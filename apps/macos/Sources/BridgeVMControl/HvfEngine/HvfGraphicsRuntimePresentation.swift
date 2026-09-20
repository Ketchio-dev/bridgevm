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
        case .nextLaunchBasic: return "다음 시작: 기본 디스플레이 · 3D 제외"
        case .nextLaunchExperimental: return "다음 시작: Graphics Lab · 실험적 3D"
        case .runningBasic: return "실행 그래픽: 기본 디스플레이 · 3D 제외"
        case .runningExperimental: return "실행 그래픽: Graphics Lab · 실험적 3D"
        case .runningUnverified: return "실행 그래픽: 기존 세션 · 모드 확인 불가"
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
