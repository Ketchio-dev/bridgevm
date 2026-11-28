import SwiftUI

struct HvfWindowsReadinessCard: View {
    let report: HvfWindowsReadinessReport

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    Label(
                        report.launchReady ? "Ready to start Windows" : "Can’t start Windows yet (\(report.launchBlockers.count))",
                        systemImage: report.launchReady ? "checkmark.circle.fill" : "xmark.octagon.fill"
                    )
                    .foregroundColor(report.launchReady ? .green : .red)
                    ForEach(report.launchBlockers) { issue in
                        Text(issue.summary)
                            .font(.caption)
                            .foregroundColor(.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                if !report.productLimitations.isEmpty {
                    Divider()
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Limitations").font(.subheadline.weight(.semibold))
                        ForEach(report.productLimitations, id: \.self) { limitation in
                            Text(limitation)
                                .font(.caption)
                                .foregroundColor(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(6)
        } label: {
            Label("Windows startup readiness", systemImage: "checklist")
        }
    }
}
