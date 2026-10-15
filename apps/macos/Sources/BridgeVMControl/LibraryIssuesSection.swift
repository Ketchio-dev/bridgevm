import SwiftUI

struct LibraryIssuesSection: View {
    let issues: [VMLibraryIssue]
    var compact = false
    @State private var showingAll = false

    var body: some View {
        Group {
            if compact {
                Button { showingAll = true } label: {
                    Label("확인이 필요한 라이브러리 항목 \(issues.count)개 · 자세히 보기", systemImage: "exclamationmark.triangle")
                }
                .buttonStyle(.plain).font(.caption).foregroundStyle(.orange)
                .accessibilityIdentifier("bridgevm.library.issues.show-all")
            } else {
                section
            }
        }
        .sheet(isPresented: $showingAll) {
            LibraryIssuesDetails(issues: issues) { showingAll = false }
        }
    }

    private var section: some View {
        Section("라이브러리 경고") {
            Label("읽지 못한 VM 설정 \(issues.count)개", systemImage: "exclamationmark.triangle.fill")
                .foregroundColor(.orange)
            ForEach(Array(issues.prefix(3))) { issue in
                VStack(alignment: .leading, spacing: 2) {
                    Text(URL(fileURLWithPath: issue.path).deletingLastPathComponent().lastPathComponent)
                        .font(.caption.bold())
                    Text(issue.message).font(.caption2).foregroundColor(.secondary)
                }
                .help(issue.path)
            }
            Button("모든 경고 보기…") { showingAll = true }
                .accessibilityIdentifier("bridgevm.library.issues.show-all")
        }
    }
}
