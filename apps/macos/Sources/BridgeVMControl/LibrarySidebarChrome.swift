import SwiftUI

struct LibraryBrand: View {
    var body: some View {
        HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(Color.blue, lineWidth: 3)
                    .frame(width: 25, height: 28).offset(x: -6, y: -5)
                RoundedRectangle(cornerRadius: 4)
                    .fill(LibraryAppearance.surface)
                    .frame(width: 25, height: 28).offset(x: 6, y: 5)
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(Color.purple, lineWidth: 3)
                    .frame(width: 25, height: 28).offset(x: 6, y: 5)
            }
            .frame(width: 42, height: 42)
            .accessibilityHidden(true)
            Text("BridgeVM").font(.system(size: 23, weight: .bold))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 22)
    }
}

struct LibraryHostSummary: View {
    @ObservedObject var library: LibraryModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label("이 Mac의 용량", systemImage: "desktopcomputer")
                .font(.callout.weight(.semibold))
            HStack(spacing: 18) {
                LibraryMetadataLabel(title: "메모리", value: "\(Int(library.hostMemGiB.rounded())) GiB", symbol: "memorychip")
                LibraryMetadataLabel(title: "CPU", value: "\(library.hostCPU) 코어", symbol: "cpu")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .modifier(LibraryCardSurface())
        .help("호스트의 전체 메모리와 사용 가능한 프로세서 수입니다. 실시간 사용률이 아닙니다.")
    }
}

struct LibraryEngineLegend: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            row(.green, "리눅스 (Fast VZ)", "사용 가능")
            row(.gray, "윈도우 (QEMU)", "준비중")
            row(.orange, "윈도우 (HVF 엔진)", "실험")
        }
        .padding(.horizontal, 6)
    }

    private func row(_ color: Color, _ title: String, _ state: String) -> some View {
        HStack(spacing: 7) {
            Circle().fill(color).frame(width: 6, height: 6).accessibilityHidden(true)
            Text(title).font(.caption)
            Spacer(minLength: 4)
            Text(state).font(.caption).foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}
