import SwiftUI

extension LibraryRetainedControlDescriptor {
    var kindLabel: String {
        switch self {
        case .runtime, .generic: return "실행"
        case .install: return "설치"
        }
    }

    var sortKey: [String] {
        let kind: String
        switch self {
        case .install: kind = "0"
        case .runtime: kind = "1"
        case .generic: kind = "2"
        }
        return [config.slug, kind, config.name, config.bundlePath, String(describing: identity)]
    }

    var statusLabel: String {
        switch self {
        case let .install(_, session): return session.stage.label
        case let .generic(_, model): return model.statusNote.isEmpty ? (model.running ? "실행 관찰됨" : "중지됨") : model.statusNote
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

    @ViewBuilder
    func detail(dismiss: @escaping () -> Void) -> some View {
        switch self {
        case let .runtime(config, session):
            RetainedRuntimeControlView(config: config, session: session) {
                dismiss()
            }
        case let .generic(config, model):
            RetainedGenericControlView(config: config, model: model) { dismiss() }
        case let .install(config, session):
            RetainedInstallControlView(config: config, session: session) {
                dismiss()
            }
        }
    }
}
