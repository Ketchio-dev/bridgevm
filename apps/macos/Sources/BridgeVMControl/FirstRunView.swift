import SwiftUI
#if canImport(AppKit)
import AppKit
#endif

/// D2 first-run import wizard: shown when the library has no VMs. Lets the user
/// register an existing installed Windows HVF bundle (disk + 64 MiB UEFI vars +
/// optional vTPM state dir) and select it. Import-only — ISO install and
/// from-scratch creation stay in their own flows.
struct FirstRunView: View {
    @ObservedObject var library: LibraryModel

    @State private var displayName = "Windows 11"
    @State private var diskPath = ""
    @State private var varsPath = ""
    @State private var vtpmPath = ""
    @State private var memGiB = 6
    @State private var cpuCount = 4

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("BridgeVM에 오신 것을 환영합니다")
                .font(.largeTitle).bold()
            Text("기존 Windows VM을 가져오세요. 가져온 뒤 VM 화면의 ‘시작’을 누르세요.")
                .foregroundStyle(.secondary)

            if !library.firstRunImportBusy && library.firstRunImport.publishedConfig == nil {
                Form {
                    TextField("이름", text: $displayName)
                    pathRow("디스크 이미지 (.raw)", $diskPath, chooseDirectory: false)
                    pathRow("UEFI vars 파일 (64 MiB)", $varsPath, chooseDirectory: false)
                    pathRow("vTPM 상태 폴더 (선택)", $vtpmPath, chooseDirectory: true)
                    Stepper("RAM: \(memGiB) GiB", value: $memGiB, in: 2...64)
                    Stepper("CPU: \(cpuCount)", value: $cpuCount, in: 1...16)
                }
                .frame(maxWidth: 620)
            }

            FirstRunImportStatusView(library: library,
                canImport: !diskPath.isEmpty && !varsPath.isEmpty, importAction: runImport)
            Spacer()
        }
        .padding(32)
        .frame(minWidth: 700, minHeight: 520)
    }

    @ViewBuilder
    private func pathRow(_ label: String, _ binding: Binding<String>, chooseDirectory: Bool)
        -> some View
    {
        HStack {
            TextField(label, text: binding)
            Button("선택…") { pick(binding, chooseDirectory: chooseDirectory) }
        }
    }

    private func pick(_ binding: Binding<String>, chooseDirectory: Bool) {
        #if canImport(AppKit)
        let panel = NSOpenPanel()
        panel.canChooseFiles = !chooseDirectory
        panel.canChooseDirectories = chooseDirectory
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            binding.wrappedValue = url.path
        }
        #endif
    }

    private func runImport() {
        guard !library.firstRunImportBusy else { return }
        let inputs = FirstRunImport.Inputs(
            displayName: displayName,
            diskPath: diskPath,
            varsPath: varsPath,
            vtpmStateDir: vtpmPath.isEmpty ? nil : vtpmPath,
            memMiB: memGiB * 1024,
            cpuCount: cpuCount)
        Task { await library.importExistingHvfVM(inputs) }
    }
}
