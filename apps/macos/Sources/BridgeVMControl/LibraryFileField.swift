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
                Button("선택…") { Self.choose(with: NSOpenPanel(), directory: chooseDirectory, path: $path) }
                    .accessibilityLabel("\(title) 선택").accessibilityIdentifier("\(accessibilityIdentifier).choose")
            }
            Text(detail).font(.caption).foregroundStyle(.secondary)
        }
    }

    /// T19 presses these choosers through Accessibility; like the create sheet, they must not hold
    /// that request in a modal loop (FileSelection). Cancel keeps the typed or earlier path.
    @MainActor
    static func choose(with panel: FileSelectionPanel, directory: Bool, path: Binding<String>) {
        FileSelection.present(panel, directories: directory) { path.wrappedValue = $0.path }
    }
}
