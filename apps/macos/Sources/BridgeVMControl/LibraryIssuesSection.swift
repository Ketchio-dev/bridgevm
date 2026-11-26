import SwiftUI

struct LibraryIssuesSection: View {
    let issues: [VMLibraryIssue]
    var compact = false
    @State private var showingAll = false

    var body: some View {
        Group {
            if compact {
                Button { showingAll = true } label: {
                    Label("Library items needing attention: \(issues.count) · View details", systemImage: "exclamationmark.triangle")
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
        Section("Library warnings") {
            Label("Unreadable VM configurations: \(issues.count)", systemImage: "exclamationmark.triangle.fill")
                .foregroundColor(.orange)
            ForEach(Array(issues.prefix(3))) { issue in
                VStack(alignment: .leading, spacing: 2) {
                    Text(issue.entry.lastPathComponent)
                        .font(.caption.bold())
                    Text(issue.message).font(.caption2).foregroundColor(.secondary)
                }
                .help(issue.path)
            }
            Button("Show all warnings…") { showingAll = true }
                .accessibilityIdentifier("bridgevm.library.issues.show-all")
        }
    }
}
