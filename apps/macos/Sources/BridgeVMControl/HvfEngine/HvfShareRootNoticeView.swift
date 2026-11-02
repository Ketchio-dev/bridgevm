import SwiftUI

struct HvfShareRootNoticeView: View {
    @ObservedObject var notice: HvfShareRootNotice

    var body: some View {
        if notice.isBlocked {
            VStack(alignment: .leading, spacing: 4) {
                Label("공유 폴더를 확인할 수 없어 일부 동기화를 기다리고 있습니다.",
                      systemImage: "folder.badge.questionmark")
                    .font(.callout).foregroundStyle(.orange)
                Text("원래 폴더의 연결을 확인하세요. 다른 폴더를 쓰려면 VM을 종료한 뒤 다시 선택하세요.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .textSelection(.enabled)
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("bridgevm.runtime.share.notice")
        }
    }
}
