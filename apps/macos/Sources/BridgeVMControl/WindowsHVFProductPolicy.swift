import Foundation

enum WindowsHVFProductPolicy {
    struct StagedISO {
        let path: String
        let sha256: String
    }

    static func requiresTemplate(_ mode: CreateVMSheet.Mode) -> Bool {
        switch mode {
        case .ubuntu, .iso, .windows: return true
        case .windowsHVF, .windowsHVFInstall: return false
        }
    }

    static func stageISO(_ selectedPath: String, in bundlePath: String) -> StagedISO? {
        let source = URL(fileURLWithPath: selectedPath)
            .resolvingSymlinksInPath().standardizedFileURL.path
        let destination = bundlePath + "/disks/installer.iso"
        guard VMLibrary.cloneOrCopyFile(from: source, to: destination),
              let sha256 = HvfWindowsInstallCacheIdentity.sha256File(destination) else { return nil }
        return StagedISO(path: destination, sha256: sha256)
    }
}
