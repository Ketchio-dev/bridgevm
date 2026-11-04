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
    /// Windows clock shows local time, and the Mac's region as UserLocale and
    /// SystemLocale, so formats and non-Unicode programs (Korean code page 949)
    /// match it. InputLocale and UILanguage stay as written: typed text arrives
    /// already composed, and the display language follows the Windows image.
    /// Its hash joins the install source cache key like any staged answer file.
    /// nil (the template's defaults) only when the template cannot be read or
    /// the copy cannot be written.
    static func stageHostRegion(
        in bundlePath: String, timeZone: TimeZone = .current, locale: Locale = .current, repoRoot: URL
    ) -> StagedAnswerFile? {
        let template = repoRoot.appendingPathComponent("scripts/win-assets/unattend.xml")
        guard let data = try? HvfWindowsInstallDurability.readRegularFile(
                template, maximumBytes: 1_048_576),
              let text = String(data: data, encoding: .utf8) else { return nil }
        let zone = windowsTimeZone(for: timeZone.identifier)
        let name = windowsLocale(for: locale)
        let answer = text.replacingOccurrences(
            of: "<TimeZone>UTC</TimeZone>", with: "<TimeZone>\(zone)</TimeZone>")
            .replacingOccurrences(
                of: "<SystemLocale>en-US</SystemLocale>", with: "<SystemLocale>\(name)</SystemLocale>")
            .replacingOccurrences(
                of: "<UserLocale>en-US</UserLocale>", with: "<UserLocale>\(name)</UserLocale>")
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

    /// Windows locale name for the Mac's region: the Mac's language there when
    /// Windows has that pair (fr_CA is fr-CA), else the region's main locale
    /// (en_KR is ko-KR: an app without a Korean localization sees the English
    /// language with the Korean region), else en-US for regions not in the table.
    static func windowsLocale(for locale: Locale) -> String {
        guard let region = locale.region?.identifier else { return "en-US" }
        if let language = locale.language.languageCode?.identifier,
           WindowsLocales.otherLanguages.contains("\(language)-\(region)") {
            return "\(language)-\(region)"
        }
        return WindowsLocales.byRegion[region] ?? "en-US"
    }
}
