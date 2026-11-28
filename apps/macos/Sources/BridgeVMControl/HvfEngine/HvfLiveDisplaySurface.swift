#if canImport(AppKit)
import SwiftUI

/// The VM window's content. Nothing of the app's own sits over Windows but the notices and the
/// paused overlay: the VM's actions, Guest windows included, are in the menu bar's Virtual Machine menu.
struct HvfLiveDisplaySurface: View {
    @ObservedObject var session: HvfEngineSession
    @ObservedObject var window: HvfDisplayWindowState

    init(session: HvfEngineSession, window: HvfDisplayWindowState? = nil) {
        _session = ObservedObject(wrappedValue: session)
        _window = ObservedObject(wrappedValue: window ?? HvfDisplayWindowState(session: session))
    }

    var body: some View {
        HvfFramebufferView(session: session)
            .background(Color.black)
            .overlay { HvfStartupDisplayOverlay(session: session, window: window) }
            .overlay { HvfPausedDisplayOverlay(session: session) }
            .overlay(alignment: .top) {
                VStack(spacing: 0) {
                    HvfRunFailureStrip(session: session)
                    HvfStopWaitStrip(session: session)
                    HvfAgentSilenceStrip(notice: session.agentSilenceNotice)
                    HvfInputStoppedStrip(session: session)
                    HvfGuestToolsStrip(session: session)
                    HvfDisplayDriverNoticeView(session: session)
                    HvfDiskSyncNoticeView(notice: session.diskSyncNotice)
                }
            }
            .overlay(alignment: .bottom) { HvfFileDropNoticeView(drops: session.fileDrops) }
            .sheet(isPresented: $window.showsSettings) { HvfVMSettingsSheet(session: session) }
            .sheet(isPresented: $window.showsWindowInventory) { HvfWindowInventoryPanel(session: session) }
            .sheet(isPresented: $window.showsAllApps) {
                HvfGuestStartAppsPanel(apps: window.startApps) { [window] in window.perform(.startApp($0)) }
            }
    }
}
#endif
