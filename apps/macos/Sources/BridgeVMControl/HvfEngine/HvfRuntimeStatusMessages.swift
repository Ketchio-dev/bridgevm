import SwiftUI

struct HvfRuntimeStatusMessages: View {
    @ObservedObject var session: HvfEngineSession
    let refusal: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(refusal ?? session.guiStartFailureText ?? "시작 오류 없음")
                .font(.callout).foregroundColor((refusal ?? session.guiStartFailureText) == nil ? Color.secondary : Color.red).textSelection(.enabled)
                .accessibilityIdentifier("bridgevm.windows.runtime.start.failure")
                .accessibilityValue(refusal ?? session.guiStartFailureText ?? "none")
            HvfAgentSilenceNoticeView(notice: session.agentSilenceNotice)
            HvfShareRootNoticeView(notice: session.shareRootNotice)
            HvfMicrophoneNoticeView(session: session)
            HvfAgentUpdateNoticeView(notice: session.agentUpdateNotice)
            HvfClipboardImageNoticeView(notice: session.clipboardImageNotice)
            HvfGuestLinkNoticeView(notice: session.guestLinkNotice)
        }
    }
}
