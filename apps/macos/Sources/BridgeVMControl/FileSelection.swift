import AppKit
import UniformTypeIdentifiers

@MainActor
protocol FileSelectionPanel: AnyObject {
    var allowsMultipleSelection: Bool { get set }
    var canChooseDirectories: Bool { get set }
    var canChooseFiles: Bool { get set }
    var allowedContentTypes: [UTType] { get set }
    var url: URL? { get }
    func begin(completionHandler handler: @escaping (NSApplication.ModalResponse) -> Void)
}

extension NSOpenPanel: FileSelectionPanel {}

@MainActor
enum FileSelection {
    static func choose(directories: Bool, extensions: [String]? = nil,
                       selection: @escaping (URL) -> Void) {
        present(NSOpenPanel(), directories: directories, extensions: extensions, selection: selection)
    }

    /// A synchronous runModal leaves the AXPress request on the stack until
    /// the chooser closes. The automation client cannot then supply its path.
    /// Return to the accessibility caller while the user chooses asynchronously.
    static func present(_ panel: FileSelectionPanel, directories: Bool,
                        extensions: [String]? = nil, selection: @escaping (URL) -> Void) {
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = directories
        panel.canChooseFiles = !directories
        if let extensions {
            panel.allowedContentTypes = extensions.compactMap { UTType(filenameExtension: $0) }
        }
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            selection(url)
        }
    }
}
