import SwiftUI
import AppKit

struct LibraryFileField: View {
    let title: String; let detail: String; let accessibilityIdentifier: String
    @Binding var path: String
    var chooseDirectory = false

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Label(title, systemImage: chooseDirectory ? "folder" : "doc")
                .font(.callout.weight(.semibold))
            HStack(spacing: 10) {
                TextField(chooseDirectory ? "폴더 경로" : "파일 경로", text: $path)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel(title).accessibilityIdentifier("\(accessibilityIdentifier).path")
                    .help(path.isEmpty ? detail : path)
                Button("선택…", action: pick)
                    .accessibilityLabel("\(title) 선택").accessibilityIdentifier("\(accessibilityIdentifier).choose")
            }
            Text(detail).font(.caption).foregroundStyle(.secondary)
        }
    }

    private func pick() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = !chooseDirectory
        panel.canChooseDirectories = chooseDirectory
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url { path = url.path }
    }
}
