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
            if runtimeActive {
                Button("Stop", systemImage: "stop.fill") {
                    LibraryDashboardPrimaryAction.stop(config: config, model: model, library: library)
                }
                .disabled(!presentation.isEnabled)
                .help(presentation.guidance)
                .accessibilityHint(presentation.guidance)
            }
        }
    }
}

extension LibraryDashboardRuntimeStopControl {
    var presentation: HvfRuntimeStopPresentation {
        .make(runtimeActive: runtimeActive, lifecycleBusy: lifecycleBusy,
              ownsRuntime: session.ownedProcessIdentity != nil)
    }
}
