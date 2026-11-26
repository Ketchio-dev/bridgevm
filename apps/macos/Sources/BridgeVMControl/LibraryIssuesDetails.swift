import SwiftUI

struct LibraryIssuesDetails: View {
    let issues: [VMLibraryIssue]
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label("Library warnings (\(issues.count))", systemImage: "exclamationmark.triangle")
                .font(.title2.weight(.semibold))
            Text("BridgeVM found these issues while reading your library.")
                .foregroundStyle(.secondary)
            List(issues) { issue in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(issue.entry.lastPathComponent).font(.headline)
                        Spacer()
                        Button("Show in Finder") {
                            NSWorkspace.shared.activateFileViewerSelecting([issue.entry])
                        }
                        .buttonStyle(.borderless)
                    }
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
                Button("Close", action: dismiss)
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("bridgevm.library.issues.close")
            }
        }
        .padding(24)
        .frame(width: 600, height: 420)
    }
}
