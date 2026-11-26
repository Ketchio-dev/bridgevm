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
                TextField(chooseDirectory ? "Folder path" : "File path", text: $path)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel(title).accessibilityIdentifier("\(accessibilityIdentifier).path")
                    .help(path.isEmpty ? detail : path)
                Button(chooseDirectory ? "Choose or drop folder…" : "Choose or drop file…") {
                    dropRejected = false
                    Self.choose(with: NSOpenPanel(), directory: chooseDirectory, path: $path)
                }
                .fixedSize()
                .accessibilityLabel("Choose \(title)").accessibilityIdentifier("\(accessibilityIdentifier).choose")
                .accessibilityHint(chooseDirectory ? "You can also drop a folder here from Finder." : "You can also drop a file here from Finder.")
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
                Label(chooseDirectory ? "Drop one folder here from Finder." : "Drop one file here from Finder.",
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
        guard let url = FileSelection.singleLocalURL(urls) else { return false }
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
