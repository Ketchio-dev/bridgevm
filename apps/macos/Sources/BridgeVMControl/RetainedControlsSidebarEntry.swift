import SwiftUI

struct RetainedControlsSidebarEntry: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        if !library.retainedControlRecords.isEmpty {
            Section("진행 중이거나 종료된 작업") {
                ForEach(library.retainedControlRecords) { record in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(record.descriptor.config.name)
                        Text("\(record.descriptor.kindLabel) · \(record.descriptor.statusLabel)")
                            .font(.caption).foregroundColor(.secondary)
                    }
                    .tag(record.id)
                    .help("등록 목록에서 사라졌을 때 보관한 기존 작업입니다.")
                }
            }
        }
    }
}
