import Foundation

enum WindowsHVFProductPolicy {
    static let installerISO = "disks/installer.iso"

    struct StagedISO {
        let path: String
        let sha256: String
    }

    static func stageISO(_ selectedPath: String, in bundlePath: String) -> StagedISO? {
        let source = URL(fileURLWithPath: selectedPath)
            .resolvingSymlinksInPath().standardizedFileURL.path
        let destination = bundlePath + "/" + installerISO
        guard VMLibrary.cloneOrCopyFile(from: source, to: destination),
              let sha256 = HvfWindowsInstallCacheIdentity.sha256File(destination) else { return nil }
        return StagedISO(path: destination, sha256: sha256)
    }
}
