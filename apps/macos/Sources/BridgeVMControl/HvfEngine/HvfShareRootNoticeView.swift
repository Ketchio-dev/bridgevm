import SwiftUI

struct HvfShareRootNoticeView: View {
    @ObservedObject var notice: HvfShareRootNotice

    var body: some View {
        if let shown = notice.displayed {
            VStack(alignment: .leading, spacing: 4) {
                Label {
                    Text(verbatim: shown.text)
                } icon: {
                    Image(systemName: "folder.badge.questionmark").foregroundStyle(.orange)
                }
                .font(.callout)
                Text(shown.help)
                    .font(.caption).foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bridgevm.runtime.share.notice")
        }
        if let copy = notice.lastRecoveredCopy {
            Label {
                VStack(alignment: .leading, spacing: 4) {
                    Text("The Windows version was saved beside the original file.")
                        .font(.callout)
                    Text(verbatim: copy)
                        .font(.body.weight(.medium))
                }
            } icon: {
                Image(systemName: "doc.on.doc")
                    .foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bridgevm.runtime.share.recoveredCopy")
        }
    }
}
