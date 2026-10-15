import SwiftUI

struct LibraryIssuesDetails: View {
    let issues: [VMLibraryIssue]
    let dismiss: () -> Void

    var body: some View {
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
                Button("닫기", action: dismiss)
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("bridgevm.library.issues.close")
            }
        }
        .padding(24)
        .frame(width: 600, height: 420)
    }
}
