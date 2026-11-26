import SwiftUI

struct FirstRunWelcomeView: View {
    let createAction: () -> Void
    let importAction: () -> Void
    let dropAction: (FirstRunMediaChoice, URL) -> Bool

    @ScaledMetric(relativeTo: .body) private var minimumCardWidth = 266.0

    var body: some View {
        GeometryReader { area in
            ScrollView {
                choices
                    .frame(maxWidth: 720, alignment: .leading)
                    .padding(28)
                    .frame(maxWidth: .infinity, minHeight: area.size.height)
            }
        }
    }

    private var choices: some View {
        VStack(alignment: .leading, spacing: 26) {
            VStack(alignment: .leading, spacing: 12) {
                LibraryVMEmblem(engine: .hvfEngine)
                Text("첫 번째 가상 머신 추가")
                    .font(.largeTitle.bold())
                    .accessibilityAddTraits(.isHeader)
                Text("Engineering Preview · 3D 가속 없음")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            LazyVGrid(columns: [GridItem(.adaptive(minimum: minimumCardWidth), spacing: 16)],
                      alignment: .leading, spacing: 16) {
                FirstRunChoiceCard(title: "Windows 새로 설치", symbol: "opticaldisc",
                    description: "Windows ARM64 설치 ISO를 여기에 놓거나 클릭해 설치를 준비하세요.",
                    choice: .installer, action: createAction, dropAction: dropAction)
                    .accessibilityIdentifier("bridgevm.first-run.create")
                FirstRunChoiceCard(title: "기존 디스크 가져오기", symbol: "externaldrive",
                    description: "Windows가 설치된 RAW, QCOW2, 고정 크기 VHD 또는 VHDX 파일을 여기에 놓거나 클릭해 가져오세요.",
                    choice: .disk, action: importAction, dropAction: dropAction)
                    .accessibilityIdentifier("bridgevm.first-run.import")
            }
        }
    }
}

private struct FirstRunChoiceCard: View {
    let title: String
    let symbol: String
    let description: String
    let choice: FirstRunMediaChoice
    let action: () -> Void
    let dropAction: (FirstRunMediaChoice, URL) -> Bool

    @Environment(\.colorSchemeContrast) private var contrast
    @FocusState private var focused: Bool
    @State private var hovered = false
    @State private var targeted = false
    @State private var rejected = false
    private var prominent: Bool { choice == .installer }

    var body: some View {
        let accent = LibraryAppearance.accent
        let shape = RoundedRectangle(cornerRadius: LibraryAppearance.cornerRadius)
        Button(action: action) {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Image(systemName: symbol)
                        .font(.title2.weight(.medium))
                        .foregroundStyle(prominent ? accent : Color.secondary)
                        .frame(width: 46, height: 46)
                        .background((prominent ? accent : Color.secondary).opacity(0.12),
                                    in: RoundedRectangle(cornerRadius: 12))
                    Spacer()
                    Image(systemName: targeted ? "arrow.down.circle.fill" : "arrow.right")
                        .font(.body.weight(.semibold))
                        .foregroundStyle(prominent || hovered || focused ? accent : Color.secondary)
                }
                VStack(alignment: .leading, spacing: 6) {
                    Text(title).font(.title3.weight(.semibold))
                    if rejected {
                        Label(choice.rejection, systemImage: "exclamationmark.circle").foregroundStyle(.red)
                    } else {
                        Text(description).foregroundStyle(.secondary)
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(22)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .background(LibraryAppearance.surface, in: shape)
            .overlay {
                shape.strokeBorder(focused || hovered || targeted ? accent : borderColor,
                                   lineWidth: focused || targeted ? 3 : 1)
                    .allowsHitTesting(false)
            }
            .contentShape(shape)
        }
        .buttonStyle(.plain)
        .focused($focused)
        .onHover { hovered = $0 }
        .dropDestination(for: URL.self) { urls, _ in
            let accepted = choice.droppedFile(urls).map { dropAction(choice, $0) } ?? false
            rejected = !accepted
            return accepted
        } isTargeted: { targeted = $0 }
        .accessibilityLabel(title)
        .accessibilityHint(rejected ? choice.rejection : description)
    }

    private var borderColor: Color {
        let color = prominent ? LibraryAppearance.accent : Color.primary
        let opacity = contrast == .increased ? 0.6 : (prominent ? 0.34 : 0.12)
        return color.opacity(opacity)
    }
}
