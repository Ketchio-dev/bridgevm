import SwiftUI

struct LibraryEmptyState: View {
    var body: some View {
        ContentUnavailableView("가상 머신을 선택하세요", systemImage: "desktopcomputer",
            description: Text("목록에서 가상 머신을 선택하면 상태와 설정을 볼 수 있습니다."))
    }
}
