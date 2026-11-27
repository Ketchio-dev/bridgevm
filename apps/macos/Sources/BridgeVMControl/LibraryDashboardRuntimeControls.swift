import SwiftUI

struct LibraryDashboardRuntimeControls: View {
    @Environment(\.libraryWindowPresentation) private var presentation
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    @ObservedObject var session: HvfEngineSession
    let showAdvanced: () -> Void
    var showSettings: (() -> Void)? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Open Display stays available while Windows shuts down: the window is where an app
            // asking to save, or holding up the shutdown, shows (HvfGuestStopCommand's advice).
            LibraryDashboardActionBar(runtimeActive: runtimeActive, lifecycleBusy: session.runtimeStartupWorkerPending,
                open: { LibraryDashboardPrimaryAction.perform(config: config, model: model, library: library) },
                showAdvanced: showAdvanced, showSettings: showSettings, refresh: model.refresh,
                clone: { presentation?.wrappedValue.cloneSource = library.requestWindowsClone(config) },
                cloneDisabled: runtimeActive || library.cloningSlugs.contains(config.slug))
            HStack(alignment: .top, spacing: 8) {
                LibraryDashboardRuntimeStopControl(config: config, model: model, library: library,
                    session: session, runtimeActive: runtimeActive, lifecycleBusy: lifecycleBusy)
                HvfRestartControl(session: session, identifier: "bridgevm.dashboard.runtime") { enabled, press in
                    if runtimeActive {
                        Button("Restart", systemImage: "arrow.counterclockwise", action: press)
                            .disabled(!enabled || lifecycleBusy)
                            .accessibilityIdentifier("bridgevm.dashboard.runtime.restart")
                    }
                }
                HvfPauseControl(session: session, identifier: "bridgevm.dashboard.runtime") { enabled, paused, press in
                    if runtimeActive {
                        Button(HvfPauseControl<EmptyView>.title(paused: paused),
                               systemImage: HvfPauseControl<EmptyView>.symbol(paused: paused), action: press)
                            .disabled(!enabled || lifecycleBusy)
                            .accessibilityIdentifier("bridgevm.dashboard.runtime.pause")
                    }
                }
            }
            if let pending = session.runtimePendingWorkText {
                Label(pending, systemImage: "hourglass").font(.caption).foregroundStyle(.secondary)
            }
            if let failure = session.runtimeFailureText {
                Text(failure).font(.caption).foregroundStyle(.red).textSelection(.enabled)
            }
            HvfAgentSilenceNoticeView(notice: session.agentSilenceNotice)
            HvfGuestToolsAction(session: session)
            HvfShareRootNoticeView(notice: session.shareRootNotice)
            HvfDiskSyncNoticeView(notice: session.diskSyncNotice)
            HvfPortForwardNoticeView(notice: session.portForwardNotice)
            HvfMicrophoneNoticeView(session: session)
            HvfAgentUpdateNoticeView(notice: session.agentUpdateNotice)
            HvfClipboardImageNoticeView(notice: session.clipboardImageNotice)
        }
        .onChange(of: session.runtimeStartupWorkerPending) { wasPending, isPending in
            guard wasPending, !isPending else { return }
            if let failure = session.guiStartFailureText { model.statusNote = failure }
            model.refreshStatus()
        }
    }

    private var runtimeActive: Bool { !session.runnerDiedWithoutCleanup && (model.running || session.hasActiveRuntimeWork) }
    private var lifecycleBusy: Bool {
        session.runtimeStartupWorkerPending || session.connectionState == .stopping
    }
}
