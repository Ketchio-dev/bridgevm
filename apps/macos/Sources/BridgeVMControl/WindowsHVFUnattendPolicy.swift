import Foundation

enum WindowsHVFUnattendPolicy {
    struct StagedAnswerFile {
        let path: String
        let sha256: String
    }

    static func stage(_ selectedPath: String, in bundlePath: String) -> StagedAnswerFile? {
        let source = URL(fileURLWithPath: selectedPath).standardizedFileURL
        guard source.path.hasPrefix("/"),
              let values = try? source.resourceValues(
                forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]),
              values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 1_048_576 else { return nil }
        let destination = bundlePath + "/metadata/windows-install-unattend.xml"
        guard VMLibrary.cloneOrCopyFile(from: source.path, to: destination),
              let sha256 = HvfWindowsInstallCacheIdentity.sha256File(destination) else { return nil }
        return StagedAnswerFile(path: destination, sha256: sha256)
    }
}
