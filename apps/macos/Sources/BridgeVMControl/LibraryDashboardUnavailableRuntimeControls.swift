import SwiftUI

struct LibraryDashboardUnavailableRuntimeControls: View {
    let config: VMConfig
    @ObservedObject var model: ControlModel
    @ObservedObject var library: LibraryModel
    let showAdvanced: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            LibraryDashboardActionBar(runtimeActive: model.running, lifecycleBusy: model.lifecycleBusy,
                open: { LibraryDashboardPrimaryAction.perform(config: config, model: model, library: library) },
                showAdvanced: showAdvanced, refresh: model.refresh)
            if model.running {
                Button("정지", systemImage: "stop.fill") {
                    LibraryDashboardPrimaryAction.stop(config: config, model: model, library: library)
                }
                .disabled(model.lifecycleBusy)
            }
        }
    }
}
