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

    /// The app's answer file with the Mac's time zone in place of UTC, so the
    /// Windows clock shows local time. Its hash joins the install source cache
    /// key like any staged answer file. nil (the UTC default) only when the
    /// template cannot be read or the copy cannot be written.
    static func stageHostTimeZone(
        in bundlePath: String, timeZone: TimeZone = .current, repoRoot: URL
    ) -> StagedAnswerFile? {
        let template = repoRoot.appendingPathComponent("scripts/win-assets/unattend.xml")
        guard let data = try? HvfWindowsInstallDurability.readRegularFile(
                template, maximumBytes: 1_048_576),
              let text = String(data: data, encoding: .utf8) else { return nil }
        let zone = windowsTimeZone(for: timeZone.identifier)
        let answer = text.replacingOccurrences(
            of: "<TimeZone>UTC</TimeZone>", with: "<TimeZone>\(zone)</TimeZone>")
        let destination = bundlePath + "/metadata/windows-install-unattend.xml"
        guard (try? Data(answer.utf8).write(
                to: URL(fileURLWithPath: destination), options: .atomic)) != nil,
              let sha256 = HvfWindowsInstallCacheIdentity.sha256File(destination) else { return nil }
        return StagedAnswerFile(path: destination, sha256: sha256)
    }

    /// Windows time zone ID for an IANA name; UTC for zones not in the table.
    static func windowsTimeZone(for identifier: String) -> String {
        WindowsTimeZones.byIANA[identifier] ?? "UTC"
    }
}
