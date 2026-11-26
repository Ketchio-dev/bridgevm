import Foundation

enum FirstRunMediaChoice {
    case installer, disk

    @MainActor
    func droppedFile(_ urls: [URL]) -> URL? {
        guard let url = FileSelection.singleLocalURL(urls), VMLibrary.isReadableRegularFile(url.path) else { return nil }
        // RAW disks need no filename extension.
        let destination: Self = url.pathExtension.lowercased() == "iso" ? .installer : .disk
        return self == destination ? url : nil
    }

    var rejection: String {
        switch self {
        case .installer: return "Drop one Windows installer ISO here."
        case .disk: return "Drop one disk with Windows installed here. For an ISO, use Install Windows."
        }
    }
}
