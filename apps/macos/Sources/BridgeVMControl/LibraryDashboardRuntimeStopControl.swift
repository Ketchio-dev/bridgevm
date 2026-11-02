import SwiftUI

struct LibraryDashboardRuntimeStopControl: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    @ObservedObject var session: HvfEngineSession
    let runtimeActive: Bool
    let lifecycleBusy: Bool

    var body: some View {
        HvfForceStopControl(session: session, identifier: "bridgevm.dashboard.runtime") {
            DashboardActionButton(title: "정지", symbol: "stop.fill", disabled: !presentation.isEnabled) {
                LibraryDashboardPrimaryAction.stop(config: config, model: model, library: library)
            }
            .help(presentation.guidance)
            .accessibilityHint(presentation.guidance)
        }
    }
}
