import SwiftUI
#if canImport(AppKit)
import AppKit
#endif

#if canImport(AppKit)
final class BridgeVMApplicationDelegate: NSObject, NSApplicationDelegate {
    func applicationWillTerminate(_ notification: Notification) {
        BundledDaemonSupervisor.shared.stop()
    }
}
#endif

@main
struct BridgeVMApp: App {
#if canImport(AppKit)
    @NSApplicationDelegateAdaptor(BridgeVMApplicationDelegate.self) private var appDelegate
#endif
    @StateObject private var appModel = BridgeVMAppModel()

    var body: some Scene {
        WindowGroup {
            DashboardView(model: appModel.dashboardModel)
                .frame(minWidth: 1040, minHeight: 680)
        }
        .windowStyle(.titleBar)

        Settings {
            SettingsView(
                settings: appModel.settings,
                storeDoctorState: appModel.storeDoctorState,
                bundledDaemonLaunchReport: appModel.bundledDaemonLaunchReport,
                onApply: appModel.applySettings,
                onCheckStoreDoctor: appModel.checkStoreDoctor
            )
        }
    }
}
