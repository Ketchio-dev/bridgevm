import SwiftUI

/// The whole dashboard observes the same session as its runtime controls.
struct LibraryDashboardObservedRuntime: View {
    @ObservedObject var session: HvfEngineSession
    let detail: LibraryDashboardDetail

    var body: some View {
        var dashboard = detail
        dashboard.runtimeState = LibraryDashboardRuntimeState(session: session, observedRunning: detail.model.running)
        return dashboard
    }
}

enum LibraryDashboardRuntimeState: Equatable {
    case stopped, starting, booting, running, paused, stopping, working, failed

    @MainActor init(session: HvfEngineSession, observedRunning: Bool) {
        if session.hasPendingRuntimeStart { self = .starting; return }
        if session.connectionState == .stopping { self = .stopping; return }
        if session.hasPendingRuntimeMutation { self = .working; return }
        if session.guiStartFailureText != nil { self = .failed; return }
        if session.pause.holdsInput { self = .paused; return }
        switch session.connectionState {
        case .connected: self = .running
        case .booting: self = .booting
        case .timedOut: self = session.hasActiveRuntimeWork ? .booting : .failed
        case .stopping: self = .stopping
        case .stopped:
            if session.process?.isRunning == true || session.ownedProcessIdentity != nil { self = .booting }
            else if session.guiStartOperation == nil && session.lastOwnedExit == nil && observedRunning { self = .running }
            else { self = .stopped }
        }
    }

    var title: String {
        switch self {
        case .stopped: "Stopped"
        case .starting: "Preparing to start"
        case .booting: "Booting Windows"
        case .running: "Running"
        case .paused: "Paused"
        case .stopping: "Shutting down"
        case .working: "Working"
        case .failed: "Needs attention"
        }
    }

    var color: Color {
        switch self {
        case .stopped: .secondary
        case .running: .green
        case .paused: .yellow
        case .failed: .red
        default: .orange
        }
    }

    func note(_ text: String) -> String? {
        text.isEmpty || Self.transientNotes.contains(text) ? nil : text
    }

    // These action acknowledgements are superseded by the observed state above.
    static let transientNotes: Set<String> = [
        "Running", "VM exited", "Starting VM…", "Stop requested.", "VM 시작 중…", "VM 부팅 중…", "VM 정지 중…", "VM 정지 요청을 보냈습니다.",
        "실행 중", "정지됨", "VM이 종료됨"
    ]
}
