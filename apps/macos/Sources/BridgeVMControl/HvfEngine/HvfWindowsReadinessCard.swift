import SwiftUI

struct HvfWindowsReadinessCard: View {
    let report: HvfWindowsReadinessReport

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 16) {
                VStack(alignment: .leading, spacing: 6) {
                    Label(
                        report.launchReady ? "부팅 준비 완료" : "부팅 차단 \(report.launchBlockers.count)건",
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
                Divider()
                VStack(alignment: .leading, spacing: 6) {
                    Text("제품 출시 검증").font(.subheadline.weight(.semibold))
                    Text(report.releaseReady
                         ? "제품 출시 게이트도 통과했습니다."
                         : "제품 출시 차단 \(report.releaseBlockers.count)건 — 개발 VM 부팅 가능 여부와 별도입니다.")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    ForEach(report.releaseBlockers) { issue in
                        Text(issue.summary)
                            .font(.caption)
                            .foregroundColor(.orange)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                if !report.productLimitations.isEmpty {
                    Divider()
                    VStack(alignment: .leading, spacing: 6) {
                        Text("제품 제한").font(.subheadline.weight(.semibold))
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
            Label("Windows HVF Readiness", systemImage: "checklist")
        }
    }
}
