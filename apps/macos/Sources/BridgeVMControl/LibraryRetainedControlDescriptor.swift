import Combine

enum LibraryRetainedControlIdentity: Hashable {
    case runtime(ObjectIdentifier)
    case install(ObjectIdentifier)
    case generic(ObjectIdentifier)
}

@MainActor
enum LibraryRetainedControlDescriptor {
    case runtime(config: VMConfig, session: HvfEngineSession)
    case install(config: VMConfig, session: HvfWindowsInstallSession)
    case generic(config: VMConfig, model: ControlModel)

    var config: VMConfig {
        switch self {
        case let .runtime(config, _), let .install(config, _), let .generic(config, _): return config
        }
    }

    var identity: LibraryRetainedControlIdentity {
        switch self {
        case let .runtime(_, session): return .runtime(ObjectIdentifier(session))
        case let .install(_, session): return .install(ObjectIdentifier(session))
        case let .generic(_, model): return .generic(ObjectIdentifier(model))
        }
    }

    var isActive: Bool {
        switch self {
        case let .runtime(_, session): return session.hasActiveRuntimeWork
        case let .install(_, session): return session.isRunning
        case let .generic(_, model): return model.hasActiveRuntimeWork
        }
    }

    func observeState(_ change: @escaping @MainActor () -> Void) -> AnyCancellable {
        switch self {
        case let .runtime(_, session):
            return session.$connectionState.dropFirst().sink { _ in change() }
        case let .install(_, session):
            return session.$stage.dropFirst().sink { _ in change() }
        case let .generic(_, model):
            return model.objectWillChange.sink { change() }
        }
    }
}

struct LibraryRetainedControlRecord: Identifiable {
    let id: String
    let descriptor: LibraryRetainedControlDescriptor
}
