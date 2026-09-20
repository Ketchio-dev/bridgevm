import Foundation

enum HvfDisplayWindowTitle {
    static func resolve(libraryName: String?, targetDiskPath: String) -> String {
        if let libraryName, !libraryName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return libraryName
        }
        let path = targetDiskPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard path.hasPrefix("/") else { return "Windows HVF" }
        let disk = URL(fileURLWithPath: path).standardizedFileURL
        let disks = disk.deletingLastPathComponent()
        let bundle = disks.deletingLastPathComponent()
        let vm = bundle.deletingLastPathComponent().lastPathComponent
        guard disks.lastPathComponent == "disks",
              bundle.lastPathComponent == "bundle.vmbridge",
              !vm.isEmpty else { return "Windows HVF" }
        return vm
    }
}
