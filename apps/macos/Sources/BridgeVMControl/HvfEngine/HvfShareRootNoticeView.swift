import SwiftUI

struct HvfShareRootNoticeView: View {
    @ObservedObject var notice: HvfShareRootNotice

    var body: some View {
        if let shown = notice.displayed {
            VStack(alignment: .leading, spacing: 4) {
                Label(shown.text, systemImage: "folder.badge.questionmark")
                    .font(.callout).foregroundStyle(.orange)
                Text(shown.help)
                    .font(.caption).foregroundStyle(.secondary)
            }
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bridgevm.runtime.share.notice")
        }
        if let copy = notice.lastRecoveredCopy {
            Label {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Windows 편집본을 원래 파일과 같은 폴더에 보관했습니다.")
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
