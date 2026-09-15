import SwiftUI

struct FirstRunWelcomeView: View {
    let createAction: () -> Void
    let importAction: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 28) {
            HStack(spacing: 22) {
                LibraryVMEmblem(engine: .hvfEngine)
                VStack(alignment: .leading, spacing: 10) {
                    Text("BridgeVM 시작하기").font(.system(size: 30, weight: .bold))
                    Text("새 Windows 환경을 만들거나 사용하던 VM을 가져오세요.")
                        .font(.title3).foregroundStyle(.secondary)
                    Text("HVF Engine (Experimental)").font(.caption.weight(.medium)).foregroundStyle(.secondary)
                }
            }
            .padding(.vertical, 16)
            HStack(alignment: .top, spacing: 20) {
                FirstRunChoiceCard(title: "새 Windows VM 만들기", symbol: "opticaldisc",
                    description: "Windows ARM64 ISO로 설치를 준비합니다. 설치 ISO와 드라이버 파일은 다음 화면에서 선택합니다.",
                    actionTitle: "설치 준비하기", prominent: true, action: createAction)
                    .accessibilityIdentifier("bridgevm.first-run.create")
                FirstRunChoiceCard(title: "기존 VM 가져오기", symbol: "square.and.arrow.down",
                    description: "이미 설치된 Windows 디스크와 부팅 설정 파일을 선택해 라이브러리에 등록합니다.",
                    actionTitle: "파일 선택하기", prominent: false, action: importAction)
                    .accessibilityIdentifier("bridgevm.first-run.import")
            }
            Label("등록한 VM은 사이드바에서 다시 열 수 있습니다.", systemImage: "sidebar.left")
                .font(.callout).foregroundStyle(.secondary)
        }
    }
}

private struct FirstRunChoiceCard: View {
    let title: String
    let symbol: String
    let description: String
    let actionTitle: String
    let prominent: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 18) {
                Image(systemName: symbol)
                    .font(.system(size: 28, weight: .light))
                    .foregroundStyle(prominent ? Color.blue : Color.secondary)
                    .frame(width: 54, height: 54)
                    .background(.blue.opacity(prominent ? 0.1 : 0.04), in: RoundedRectangle(cornerRadius: 12))
                    .accessibilityHidden(true)
                Text(title).font(.title3.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                Text(description).font(.callout).foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 66, alignment: .topLeading)
                    .fixedSize(horizontal: false, vertical: true)
                HStack {
                    Text(actionTitle).font(.callout.weight(.semibold))
                    Spacer(minLength: 8)
                    Image(systemName: "arrow.right")
                }
                .foregroundStyle(prominent ? Color.white : Color.primary)
                .padding(12)
                .background(prominent ? Color.blue : Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 9))
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
            .modifier(LibraryCardSurface())
            .contentShape(RoundedRectangle(cornerRadius: LibraryAppearance.cornerRadius))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(title)
        .accessibilityHint(description)
    }
}
