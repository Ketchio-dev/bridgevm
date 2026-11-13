import Combine
import SwiftUI

/// A later successful sync cannot undo the earlier durability failure, so this stays until
/// dismissed or the next run; a guest reboot is not a new run.
@MainActor
final class HvfDiskSyncNotice: ObservableObject {
    static let text = "Windows 디스크를 Mac에 안전하게 저장하지 못했으니 Mac의 저장 공간과 디스크 연결을 확인한 뒤 Windows를 종료했다가 다시 시작하세요."
    @Published private(set) var isVisible = false
    @Published private(set) var finalSaveFailed = false
    private var reported = false

    func consume(_ event: BvAgentEvent) {
        if case .diskFinalSaveFailed = event { finalSaveFailed = true; return }
        guard case .diskDurabilityFailed = event, !reported else { return }
        reported = true
        isVisible = true
    }
    func dismiss() { isVisible = false }
    func reset() { reported = false; isVisible = false; finalSaveFailed = false }
}

struct HvfDiskSyncNoticeView: View {
    @ObservedObject var notice: HvfDiskSyncNotice

    var body: some View {
        if notice.isVisible {
            HvfNoticeStrip(identifier: "bridgevm.storage.durability", dismiss: { notice.dismiss() }) {
                Text(HvfDiskSyncNotice.text)
            }
        }
    }
}
