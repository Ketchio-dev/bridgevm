import SwiftUI

struct LibraryIssuesSection: View {
    let issues: [VMLibraryIssue]
    @State private var showingAll = false

    var body: some View {
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
        .sheet(isPresented: $showingAll) { details }
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label("라이브러리 경고 \(issues.count)개", systemImage: "exclamationmark.triangle")
                .font(.title2.weight(.semibold))
            Text("라이브러리를 읽는 중 아래 문제가 발생했습니다.")
                .foregroundStyle(.secondary)
            List(issues) { issue in
                VStack(alignment: .leading, spacing: 8) {
                    Text(URL(fileURLWithPath: issue.path).deletingLastPathComponent().lastPathComponent)
                        .font(.headline)
                    Text(issue.message).fixedSize(horizontal: false, vertical: true)
                    Text(issue.path).font(.caption.monospaced()).foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .textSelection(.enabled)
                .padding(.vertical, 6)
            }
            .accessibilityIdentifier("bridgevm.library.issues.list")
            HStack {
                Spacer()
                Button("닫기") { showingAll = false }
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("bridgevm.library.issues.close")
            }
        }
        .padding(24)
        .frame(width: 600, height: 420)
    }
}
