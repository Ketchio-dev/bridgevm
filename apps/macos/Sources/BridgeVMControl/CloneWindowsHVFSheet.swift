import SwiftUI

struct CloneWindowsHVFSheet: View {
    @ObservedObject var library: LibraryModel
    let config: VMConfig
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Windows HVF VM 복제").font(.title2.bold())
            Text("APFS copy-on-write로 번들을 복제합니다. 복제본은 원본의 TPM 키를 공유하지 않으며 새 TPM ID로 시작합니다. 복사된 이전 TPM 상태와 실행 증거는 보관됩니다.")
                .foregroundColor(.secondary)
            TextField("새 VM 이름", text: $name)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("취소") { dismiss() }
                Button("복제") {
                    library.cloneWindowsHVF(config, name: name)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .disabled(VMLibrary.normalizedVMName(name) == nil)
            }
        }
        .padding(20)
        .frame(width: 480)
        .onAppear { name = "\(config.name) Copy" }
    }
}
