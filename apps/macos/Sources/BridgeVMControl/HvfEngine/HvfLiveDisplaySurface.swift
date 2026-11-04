#if canImport(AppKit)
import SwiftUI

struct HvfLiveDisplaySurface: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        HvfFramebufferView(session: session)
            .background(Color.black)
            .overlay { HvfPausedDisplayOverlay(session: session) }
            .overlay(alignment: .topTrailing) { HvfWindowInventoryButton(session: session) }
            .overlay(alignment: .bottom) { HvfFileDropNoticeView(drops: session.fileDrops) }
    }
}
#endif
