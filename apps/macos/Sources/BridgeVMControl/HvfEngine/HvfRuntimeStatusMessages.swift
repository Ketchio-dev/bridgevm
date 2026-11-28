import SwiftUI

struct HvfRuntimeStatusMessages: View {
    @ObservedObject var session: HvfEngineSession
    let refusal: String?

    private var failureText: String? { refusal ?? session.runtimeFailureText }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(failureText ?? "No startup error")
                .font(.callout).foregroundColor(failureText == nil ? Color.secondary : Color.red).textSelection(.enabled)
                .accessibilityIdentifier(HvfRuntimeAccessibilityState.failureIdentifier(failureText))
            HvfSharedMemoryNoticeView(session: session)
            HvfAgentSilenceNoticeView(notice: session.agentSilenceNotice)
            HvfGuestToolsAction(session: session)
            HvfShareRootNoticeView(notice: session.shareRootNotice)
            HvfDiskSyncNoticeView(notice: session.diskSyncNotice)
            HvfMicrophoneNoticeView(session: session)
            HvfAgentUpdateNoticeView(notice: session.agentUpdateNotice)
            HvfClipboardImageNoticeView(notice: session.clipboardImageNotice)
            HvfPortForwardNoticeView(notice: session.portForwardNotice)
            HvfAudioNoticeView(notice: session.audioNotice)
            HvfGuestLinkNoticeView(notice: session.guestLinkNotice)
            HvfGuestPrintNoticeView(notice: session.printNotice)
        }
    }
}
