import SwiftUI

struct CloneWindowsHVFSheet: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    @Environment(\.dismiss) private var dismiss
    @State private var name: String

    init(library: LibraryModel, config: VMConfig) {
        self.library = library
        self.config = config
        _name = State(initialValue: Self.suggestedName(for: config.name))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text("가상 머신 복제").font(.title2.bold())
            HStack(spacing: 12) {
                LibraryVMEmblem(engine: config.engineKind, compact: true)
                VStack(alignment: .leading, spacing: 3) {
                    Text("원본 VM").font(.caption).foregroundStyle(.secondary)
                    Text(config.name).font(.headline).lineLimit(2).truncationMode(.middle)
                        .help(config.name)
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("bridgevm.clone.source")
            }
            VStack(alignment: .leading, spacing: 6) {
                Text("복제본 이름").font(.callout.weight(.medium))
                TextField("복제본 이름", text: $name)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("bridgevm.clone.name")
                if VMLibrary.normalizedVMName(name) == nil {
                    Text(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                         ? "복제본 이름을 입력하세요." : "이름을 줄이거나 줄바꿈과 제어 문자를 지워 주세요.")
                        .font(.caption).foregroundStyle(.red)
                }
            }
            Text("디스크와 부팅 설정을 별도로 복사합니다. 복제본은 새 TPM ID로 시작하며, 기존 TPM 상태와 실행 기록은 별도로 보관합니다.")
                .font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            Divider()
            HStack {
                Spacer()
                Button("취소", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("복제") {
                    library.cloneWindowsHVF(config, name: name)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(VMLibrary.normalizedVMName(name) == nil)
                .accessibilityIdentifier("bridgevm.clone.confirm")
            }
        }
        .padding(22)
        .frame(width: 480, alignment: .leading)
    }

    private static func suggestedName(for source: String) -> String {
        let suffix = " Copy"
        var prefix = String(source.prefix(VMLibrary.maximumVMNameCharacters - suffix.count))
        while !prefix.isEmpty {
            if let name = VMLibrary.normalizedVMName(prefix + suffix) { return name }
            prefix.removeLast()
        }
        return "Windows Copy"
    }
}
