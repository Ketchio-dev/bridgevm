import SwiftUI
import AppKit

struct LibraryFileField: View {
    let title: String; let detail: String; let accessibilityIdentifier: String
    @Binding var path: String
    var chooseDirectory = false
    @State private var isDropTarget = false
    @State private var dropRejected = false

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            Label(title, systemImage: chooseDirectory ? "folder" : "doc")
                .font(.callout.weight(.semibold))
            HStack(spacing: 10) {
                TextField(chooseDirectory ? "폴더 경로" : "파일 경로", text: $path)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel(title).accessibilityIdentifier("\(accessibilityIdentifier).path")
                    .help(path.isEmpty ? detail : path)
                Button(chooseDirectory ? "폴더 선택 또는 놓기" : "파일 선택 또는 놓기") {
                    dropRejected = false
                    Self.choose(with: NSOpenPanel(), directory: chooseDirectory, path: $path)
                }
                .fixedSize()
                .accessibilityLabel("\(title) 선택").accessibilityIdentifier("\(accessibilityIdentifier).choose")
                .accessibilityHint(chooseDirectory ? "Finder에서 폴더를 놓을 수 있습니다." : "Finder에서 파일을 놓을 수 있습니다.")
                .overlay {
                    if isDropTarget {
                        RoundedRectangle(cornerRadius: 6)
                            .strokeBorder(Color.accentColor, lineWidth: 2)
                            .allowsHitTesting(false)
                            .accessibilityHidden(true)
                    }
                }
            }
            if dropRejected {
                Label(chooseDirectory ? "Finder에서 폴더 한 개를 놓으세요." : "Finder에서 파일 한 개를 놓으세요.",
                      systemImage: "exclamationmark.circle")
                    .font(.caption).foregroundStyle(.red)
            } else {
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .dropDestination(for: URL.self) { urls, _ in
            let accepted = Self.selectDrop(urls, directory: chooseDirectory, path: $path)
            dropRejected = !accepted
            return accepted
        } isTargeted: { isDropTarget = $0 }
        .onChange(of: path) { _, _ in dropRejected = false }
    }

    static func selectDrop(_ urls: [URL], directory: Bool, path: Binding<String>) -> Bool {
        guard urls.count == 1, let url = urls.first, url.isFileURL else { return false }
        if let host = url.host, !host.isEmpty, host.lowercased() != "localhost" { return false }
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory),
              isDirectory.boolValue == directory else { return false }
        path.wrappedValue = url.path
        return true
    }

    /// T19 presses these choosers through Accessibility; like the create sheet, they must not hold
    /// that request in a modal loop (FileSelection). Cancel keeps the typed or earlier path.
    @MainActor
    static func choose(with panel: FileSelectionPanel, directory: Bool, path: Binding<String>) {
        FileSelection.present(panel, directories: directory) { path.wrappedValue = $0.path }
    }
}
