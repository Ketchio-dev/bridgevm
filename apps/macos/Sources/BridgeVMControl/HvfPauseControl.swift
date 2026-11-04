import SwiftUI

/// Pause or Resume for one VM, with the probe's answer under it when Pause did not happen.
@MainActor
struct HvfPauseControl<Content: View>: View {
    @ObservedObject var session: HvfEngineSession
    let identifier: String
    private let content: (_ enabled: Bool, _ paused: Bool, _ press: @escaping () -> Void) -> Content
    @State private var resultMessage: String?

    init(session: HvfEngineSession, identifier: String,
         @ViewBuilder content: @escaping (_ enabled: Bool, _ paused: Bool, _ press: @escaping () -> Void) -> Content) {
        _session = ObservedObject(wrappedValue: session)
        self.identifier = identifier; self.content = content
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            content(session.pause.showsResume || session.pauseAvailable, session.pause.showsResume) {
                resultMessage = session.togglePause().message
            }
            if let text = session.pauseNotice ?? resultMessage {
                Text(text).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier(identifier + ".pause-status")
            }
        }
        .onChange(of: session.pause) { _, _ in resultMessage = nil }
    }

    static func title(paused: Bool) -> String { paused ? "재개" : "일시 정지" }
    static func symbol(paused: Bool) -> String { paused ? "play.fill" : "pause.fill" }
}

/// A library row's run state; a paused VM shows as paused.
@MainActor
struct LibraryVMRunState: View {
    let text: String
    let color: Color
    let session: HvfEngineSession?
    let dot: CGFloat

    var body: some View {
        if let session { Observed(session: session, text: text, color: color, dot: dot) }
        else { Self.row(text, color, dot) }
    }

    @ViewBuilder fileprivate static func row(_ text: String, _ color: Color, _ dot: CGFloat) -> some View {
        Circle().fill(color).frame(width: dot, height: dot)
        Text(text).foregroundStyle(color)
    }

    private struct Observed: View {
        @ObservedObject var session: HvfEngineSession
        let text: String
        let color: Color
        let dot: CGFloat

        var body: some View {
            if session.pause == .running { LibraryVMRunState.row(text, color, dot) }
            else { LibraryVMRunState.row("일시 정지됨", .yellow, dot) }
        }
    }
}

/// The library's Pause or Resume item for a running VM's menu.
@MainActor
struct HvfPauseMenuItem: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        let paused = session.pause.showsResume
        if paused || session.pauseAvailable {
            Button { session.togglePause() } label: {
                Label(paused ? "재개" : "VM 일시정지", systemImage: paused ? "play.fill" : "pause.fill")
            }
            .accessibilityIdentifier("bridgevm.library.vm.pause")
        }
    }
}

/// A paused VM's display: the last frame dimmed, and Resume in the middle. It takes every click,
/// and the session ignores keys and pointer input until the guest runs again.
@MainActor
struct HvfPausedDisplayOverlay: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        if session.pause.holdsInput {
            ZStack {
                Color.black.opacity(0.55)
                VStack(spacing: 12) {
                    Button { _ = session.requestResume() } label: {
                        Label("재개", systemImage: "play.fill").font(.title2)
                    }
                    .buttonStyle(.borderedProminent).controlSize(.large)
                    .disabled(!session.pause.showsResume)
                    .accessibilityIdentifier("bridgevm.display.resume")
                    Text(label).font(.headline).foregroundStyle(.white)
                    if let notice = session.pauseNotice {
                        Text(notice).font(.callout).foregroundStyle(.white.opacity(0.85))
                    }
                }
            }
            .contentShape(Rectangle())
            .onTapGesture {}
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("bridgevm.display.paused")
        }
    }

    private var label: String {
        switch session.pause {
        case .pausing: return "일시 정지하는 중…"
        case .resuming: return "재개하는 중…"
        case .paused, .running: return "일시 정지됨"
        }
    }
}
