import SwiftUI

struct LibraryDashboardRuntimeControls: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    @ObservedObject var session: HvfEngineSession
    let showAdvanced: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                DashboardActionButton(title: runtimeActive ? "창 열기" : "시작",
                    symbol: "play.fill", prominent: true, disabled: lifecycleBusy) {
                    LibraryDashboardPrimaryAction.perform(config: config, model: model, library: library)
                }
                DashboardActionButton(title: "정지", symbol: "stop.fill",
                    disabled: !runtimeActive || lifecycleBusy) {
                    LibraryDashboardPrimaryAction.stop(config: config, model: model, library: library)
                }
                DashboardActionButton(title: "새로고침", symbol: "arrow.clockwise", action: model.refresh)
                DashboardActionButton(title: "복제", symbol: "plus.square.on.square",
                    disabled: runtimeActive || library.cloningSlugs.contains(config.slug)) {
                    library.requestWindowsClone(config)
                }
                DashboardActionButton(title: "고급", symbol: "gearshape", action: showAdvanced)
                    .accessibilityIdentifier("bridgevm.dashboard.advanced")
            }
            if let pending = session.runtimePendingWorkText {
                Label(pending, systemImage: "hourglass").font(.caption).foregroundStyle(.secondary)
            }
            if let failure = session.guiStartFailureText {
                Text(failure).font(.caption).foregroundStyle(.red).textSelection(.enabled)
            }
        }
        .onChange(of: session.runtimeStartupWorkerPending) { wasPending, isPending in
            guard wasPending, !isPending else { return }
            if let failure = session.guiStartFailureText { model.statusNote = failure }
            model.refreshStatus()
        }
    }

    private var runtimeActive: Bool { model.running || session.hasActiveRuntimeWork }
    private var lifecycleBusy: Bool {
        model.lifecycleBusy || session.runtimeStartupWorkerPending || session.connectionState == .stopping
    }
}
