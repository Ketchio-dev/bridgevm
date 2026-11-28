import SwiftUI

/// The Finder-drop notice over the display: progress, then done or why it failed.
struct HvfFileDropNoticeView: View {
    @ObservedObject var drops: HvfFileDropQueue

    var body: some View {
        if let notice = drops.notice {
            HStack(spacing: 8) {
                Label {
                    Text(notice.text)
                } icon: {
                    Image(systemName: notice.failed ? "exclamationmark.triangle" : "doc.on.doc")
                        .foregroundStyle(notice.failed ? Color.orange : Color.primary)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("bridgevm.runtime.filedrop.notice")
                // A failed notice stays until it is closed or the next drop replaces it.
                if notice.failed {
                    Button { drops.dismiss() } label: { Image(systemName: "xmark.circle.fill") }
                        .buttonStyle(.plain)
                        .foregroundStyle(.secondary)
                        .help("Close")
                        .accessibilityLabel("Dismiss the file copy notice")
                        .accessibilityIdentifier("bridgevm.runtime.filedrop.close")
                }
            }
            .font(.callout)
            .foregroundStyle(.primary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: Capsule())
            .padding(12)
            // It sits over the Windows taskbar: clicks on a progress notice go to Windows.
            .allowsHitTesting(notice.failed)
        }
    }
}
