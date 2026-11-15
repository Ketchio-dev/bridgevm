#if canImport(AppKit)
import SwiftUI

/// Startup and stopped windows need an explanation; a running guest stays unobstructed.
struct HvfStartupDisplayOverlay: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var window: HvfDisplayWindowState

    var body: some View {
        if session.hasPendingRuntimeStart || session.guiStartFailureText != nil || !session.hasActiveRuntimeWork {
            VStack(spacing: 16) {
                if session.hasPendingRuntimeStart {
                    ProgressView()
                    Text(session.runtimePendingWorkText ?? "Windows 시작 준비 중")
                } else {
                    Image(systemName: session.guiStartFailureText == nil ? "power" : "exclamationmark.triangle")
                        .font(.largeTitle)
                    Text(session.guiStartFailureText ?? "Windows가 종료되었습니다.")
                        .multilineTextAlignment(.center).textSelection(.enabled)
                    Button("창 닫기") { window.window?.performClose(nil) }
                        .keyboardShortcut(.cancelAction)
                }
            }
            .padding(28).frame(maxWidth: 480)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        }
    }
}
#endif
