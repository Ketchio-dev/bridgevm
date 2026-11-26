import SwiftUI

struct HvfWindowsInstallISOButton: View {
    let path: String

    var body: some View {
        let name = (path as NSString).lastPathComponent
        Button {
            FinderTarget(path: path)?.reveal()
        } label: {
            Label(name, systemImage: "folder")
        }
        .buttonStyle(.link)
        .disabled(FinderTarget(path: path) == nil)
        .help("Finder에서 설치 ISO 보기")
        .accessibilityLabel("Finder에서 \(name) 보기")
        .accessibilityIdentifier("bridgevm.windows.install.iso.reveal")
    }

    enum FinderTarget: Equatable {
        case file(URL), folder(URL)

        init?(path: String) {
            guard path.hasPrefix("/"), !path.hasSuffix("/") else { return nil }
            let file = URL(fileURLWithPath: path)
            let manager = FileManager.default
            var directory: ObjCBool = false
            if manager.fileExists(atPath: file.path, isDirectory: &directory) {
                guard !directory.boolValue else { return nil }
                self = .file(file)
            } else {
                let folder = file.deletingLastPathComponent()
                guard manager.fileExists(atPath: folder.path, isDirectory: &directory), directory.boolValue else { return nil }
                self = .folder(folder)
            }
        }

        func reveal() {
            switch self {
            case let .file(file): NSWorkspace.shared.activateFileViewerSelecting([file])
            case let .folder(folder): NSWorkspace.shared.open(folder)
            }
        }
    }
}
