import SwiftUI

struct LibraryDashboardUnavailableRuntimeControls: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    let showAdvanced: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            DashboardActionButton(title: model.running ? "창 열기" : "시작",
                symbol: "play.fill", prominent: true, disabled: model.lifecycleBusy) {
                LibraryDashboardPrimaryAction.perform(config: config, model: model, library: library)
            }
            DashboardActionButton(title: "정지", symbol: "stop.fill",
                disabled: !model.running || model.lifecycleBusy) {
                LibraryDashboardPrimaryAction.stop(config: config, model: model, library: library)
            }
            DashboardActionButton(title: "새로고침", symbol: "arrow.clockwise", action: model.refresh)
            DashboardActionButton(title: "고급", symbol: "gearshape", action: showAdvanced)
                .accessibilityIdentifier("bridgevm.dashboard.advanced")
        }
    }
}
