#if canImport(AppKit)
import SwiftUI

/// Startup and stopped windows need an explanation; a running guest stays unobstructed.
struct HvfStartupDisplayOverlay: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var window: HvfDisplayWindowState
    @State private var startRefusal: String?

    var body: some View {
        let spinnerText = session.startupDisplaySpinnerText
        if spinnerText != nil || session.guiStartFailureText != nil || !session.hasActiveRuntimeWork {
            VStack(spacing: 16) {
                if let spinnerText {
                    ProgressView()
                    Text(spinnerText)
                } else {
                    Image(systemName: session.runtimeFailureText == nil ? "power" : "exclamationmark.triangle")
                        .font(.largeTitle)
                    Text(session.runtimeFailureText ?? "Windows has shut down.")
                        .multilineTextAlignment(.center).textSelection(.enabled)
                    if let startRefusal {
                        Text(startRefusal).font(.callout).foregroundStyle(.orange).multilineTextAlignment(.center)
                    }
                    HStack {
                        Button("Close Window") { window.window?.performClose(nil) }
                            .keyboardShortcut(.cancelAction)
                        if let library = session.library, let vm = session.config.libraryContext?.config {
                            if session.guiStartFailureText == nil {
                                Button("Open Library") { ControlLibraryWindow.show(vm, in: library) }
                                    .accessibilityIdentifier("bridgevm.display.stopped.open-library")
                            }
                            // A shutdown from Windows, or a failed start, leaves this window open: start again here.
                            Button("Start Windows") { startRefusal = session.startFromStoppedWindow(vm, in: library) }
                                .buttonStyle(.borderedProminent)
                                .keyboardShortcut(.defaultAction)
                                .accessibilityIdentifier("bridgevm.display.stopped.start")
                        }
                    }
                }
            }
            .padding(28).frame(maxWidth: 480)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
            .padding(.horizontal, 16)
        }
    }
}

extension HvfEngineSession {
    /// The display window's spinner text, nil for none: a start being prepared, or Windows starting before
    /// its first frame, which would otherwise leave a plain black window while the firmware runs.
    var startupDisplaySpinnerText: String? {
        if hasPendingRuntimeStart { return runtimePendingWorkText ?? "Preparing to start Windows…" }
        guard connectionState == .booting, !displayShowedFrame, guiStartFailureText == nil else { return nil }
        return "Starting Windows…"
    }

    /// The stopped VM window's Start: the saved settings, as the library's Start uses them. Returns why it
    /// was refused, or nil once admitted.
    func startFromStoppedWindow(_ vm: VMConfig, in library: LibraryModel) -> String? {
        let saved = library.latestConfiguration(for: vm)
        guard let launch = HvfEngineConfig.libraryVM(saved, rootURL: library.rootURL,
                  e2eHostDiagnosticStopAdmitted: library.e2eHostDiagnosticStopAdmitted) else {
            return "BridgeVM couldn’t read this VM’s saved settings. Press Command-K in the library window and choose Refresh Library, then try again."
        }
        if case let .refused(reason) = requestGUIStart(configuration: launch) { return reason }
        return nil
    }
}
#endif
