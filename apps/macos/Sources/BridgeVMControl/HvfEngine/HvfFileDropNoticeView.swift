import SwiftUI

/// The Finder-drop notice over the display: progress, then done or why it failed.
struct HvfFileDropNoticeView: View {
    @ObservedObject var drops: HvfFileDropQueue

    var body: some View {
        if let notice = drops.notice {
            Label(notice.text, systemImage: notice.failed ? "exclamationmark.triangle" : "doc.on.doc")
                .font(.callout)
                .foregroundStyle(notice.failed ? Color.orange : Color.primary)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(.regularMaterial, in: Capsule())
                .padding(12)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("bridgevm.runtime.filedrop.notice")
        }
    }
}
