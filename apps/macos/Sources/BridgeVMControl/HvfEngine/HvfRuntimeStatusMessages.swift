import SwiftUI

struct HvfRuntimeStatusMessages: View {
    @ObservedObject var session: HvfEngineSession
    let refusal: String?

    private var failureText: String? { refusal ?? session.runtimeSupervisionFailureText ?? session.guiStartFailureText }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(failureText ?? "시작 오류 없음")
                .font(.callout).foregroundColor(failureText == nil ? Color.secondary : Color.red).textSelection(.enabled)
                .accessibilityIdentifier("bridgevm.windows.runtime.start.failure")
                .accessibilityValue(failureText ?? "none")
            HvfAgentSilenceNoticeView(notice: session.agentSilenceNotice)
            HvfGuestToolsAction(session: session)
            HvfShareRootNoticeView(notice: session.shareRootNotice)
            HvfDiskSyncNoticeView(notice: session.diskSyncNotice)
            HvfMicrophoneNoticeView(session: session)
            HvfAgentUpdateNoticeView(notice: session.agentUpdateNotice)
            HvfClipboardImageNoticeView(notice: session.clipboardImageNotice)
            HvfGuestLinkNoticeView(notice: session.guestLinkNotice)
            HvfGuestPrintNoticeView(notice: session.printNotice)
        }
    }
}
