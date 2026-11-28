import Foundation

enum HvfDisplayWindowTitle {
    static func resolve(libraryName: String?, targetDiskPath: String) -> String {
        if let libraryName, !libraryName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return libraryName
        }
        let path = targetDiskPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard path.hasPrefix("/") else { return "Windows" }
        let disk = URL(fileURLWithPath: path).standardizedFileURL
        let disks = disk.deletingLastPathComponent()
        let bundle = disks.deletingLastPathComponent()
        let vm = bundle.deletingLastPathComponent().lastPathComponent
        guard disks.lastPathComponent == "disks",
              bundle.lastPathComponent == "bundle.vmbridge",
              !vm.isEmpty else { return "Windows" }
        return vm
    }
}

extension HvfEngineSession {
    /// The VM's name in the library, as its display window and the quit alerts show it.
    var displayTitle: String {
        HvfDisplayWindowTitle.resolve(libraryName: config.libraryContext?.config.name,
                                      targetDiskPath: config.targetDiskPath)
    }
}
