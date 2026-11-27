import Combine
import SwiftUI

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
        case let .runtime(_, session): return session.hasActiveRuntimeWork
        case let .install(_, session): return session.isRunning
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

extension LibraryRetainedControlDescriptor {
    var kindLabel: String {
        switch self {
        case .runtime: return "Runtime"
        case .install: return "Installation"
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
            case .stopped: return "Stopped"
            case .booting: return "Starting"
            case .connected: return "Connected"
            case .stopping: return "Stopping"
            case .timedOut: return "Check the connection"
            }
        }
    }

    @ViewBuilder
    func detail(dismiss: @escaping () -> Void) -> some View {
        switch self {
        case let .runtime(config, session):
            RetainedRuntimeControlView(config: config, session: session) {
                dismiss()
            }
        case let .install(config, session):
            RetainedInstallControlView(config: config, session: session) {
                dismiss()
            }
        }
    }
}
