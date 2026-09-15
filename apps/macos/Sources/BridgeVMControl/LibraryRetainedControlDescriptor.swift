import Combine

enum LibraryRetainedControlIdentity: Hashable {
    case runtime(ObjectIdentifier)
    case install(ObjectIdentifier)
}

@MainActor
enum LibraryRetainedControlDescriptor {
    case runtime(config: VMConfig, session: HvfEngineSession)
    case install(config: VMConfig, session: HvfWindowsInstallSession)

    var config: VMConfig {
        switch self {
        case let .runtime(config, _), let .install(config, _): return config
        }
    }

    var identity: LibraryRetainedControlIdentity {
        switch self {
        case let .runtime(_, session): return .runtime(ObjectIdentifier(session))
        case let .install(_, session): return .install(ObjectIdentifier(session))
        }
    }

    var isActive: Bool {
        switch self {
        case let .runtime(_, session): return session.connectionState != .stopped
        case let .install(_, session): return session.isRunning
        }
    }

    var kindLabel: String {
        switch self {
        case .runtime: return "실행"
        case .install: return "설치"
        }
    }

    var sortKey: [String] {
        let kind: String
        switch self {
        case .install: kind = "0"
        case .runtime: kind = "1"
        }
        return [config.slug, kind, config.name, config.bundlePath, String(describing: identity)]
    }

    var statusLabel: String {
        switch self {
        case let .install(_, session): return session.stage.label
        case let .runtime(_, session):
            switch session.connectionState {
            case .stopped: return "중지됨"
            case .booting: return "시작 중"
            case .connected: return "연결됨"
            case .stopping: return "중지 중"
            case .timedOut: return "응답 확인 필요"
            }
        }
    }

    func observeState(_ change: @escaping @MainActor () -> Void) -> AnyCancellable {
        switch self {
        case let .runtime(_, session):
            return session.$connectionState.dropFirst().sink { _ in change() }
        case let .install(_, session):
            return session.$stage.dropFirst().sink { _ in change() }
        }
    }
}

struct LibraryRetainedControlRecord: Identifiable {
    let id: String
    let descriptor: LibraryRetainedControlDescriptor
}
