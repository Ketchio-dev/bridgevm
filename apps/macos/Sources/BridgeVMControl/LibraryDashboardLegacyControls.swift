import SwiftUI

struct LibraryDashboardLegacyControls: View {
    @ObservedObject var model: ControlModel
    let showAdvanced: () -> Void

    var body: some View {
        HStack(spacing: 12) {
            DashboardActionButton(title: model.running ? "창 열기" : "시작",
                symbol: "play.fill", prominent: true, disabled: model.lifecycleBusy,
                action: model.start)
            DashboardActionButton(title: "정지", symbol: "stop.fill",
                disabled: !model.running || model.lifecycleBusy, action: model.stop)
            DashboardActionButton(title: "새로고침", symbol: "arrow.clockwise", action: model.refresh)
            DashboardActionButton(title: "고급", symbol: "gearshape", action: showAdvanced)
                .accessibilityIdentifier("bridgevm.dashboard.advanced")
        }
    }
}
