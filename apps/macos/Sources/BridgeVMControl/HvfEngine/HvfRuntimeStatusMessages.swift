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
            HvfBootFailureNoticeView(notice: session.bootFailureNotice)
            HvfSharedMemoryNoticeView(session: session)
            HvfKeepAwakeNoticeView(power: session.sleepPrevention, runtime: session.sleepPreventionID)
            HvfAgentSilenceNoticeView(notice: session.agentSilenceNotice)
            HvfGuestToolsAction(session: session)
            HvfShareRootNoticeView(notice: session.shareRootNotice)
            HvfDiskSyncNoticeView(notice: session.diskSyncNotice)
            HvfLowSpaceNoticeView(notice: session.lowSpaceNotice)
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
