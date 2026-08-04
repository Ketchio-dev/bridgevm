import Foundation

/// Checks that a helper the app is about to execute is the one it shipped.
///
/// A bundled path is not by itself evidence: an app bundle is a directory a
/// user can write to, so `Contents/Helpers/swtpm` can be replaced after
/// install. What cannot be replaced without detection is the code signature,
/// so the helper is required to be signed by the same team as the app.
///
/// The check is skipped for an ad-hoc signed app (a local build has no team),
/// because refusing there would mean no local build could ever launch a VM.
/// It is enforced whenever the app itself carries a team identifier.
enum HelperCodeSignature {

    enum Rejection: Equatable {
        case notAFile(String)
        case unsigned(String)
        case teamMismatch(helper: String, expected: String, found: String?)

        var message: String {
            switch self {
            case let .notAFile(path):
                return "헬퍼 실행 파일이 없습니다: \(path)"
            case let .unsigned(path):
                return "헬퍼에 코드 서명이 없습니다: \(path)"
            case let .teamMismatch(helper, expected, found):
                return "헬퍼의 서명 팀이 앱과 다릅니다: \(helper) (앱 \(expected), 헬퍼 \(found ?? "없음"))"
            }
        }
    }

    /// Team identifier of a signed binary, or nil when it has none.
    ///
    /// Injected in tests; the real implementation shells out to `codesign`
    /// rather than linking Security, because the app already runs helpers as
    /// subprocesses and this keeps the check to one well-understood tool.
    typealias TeamReader = (String) -> String?

    static func validate(
        helperPath: String,
        appTeam: String?,
        fileManager: FileManager = .default,
        teamReader: TeamReader = readTeamIdentifier
    ) -> Rejection? {
        guard fileManager.isExecutableFile(atPath: helperPath) else {
            return .notAFile(helperPath)
        }
        // An ad-hoc signed app has no team; requiring one would break every
        // local build. The signature is still required to exist.
        guard let appTeam, !appTeam.isEmpty else { return nil }

        let helperTeam = teamReader(helperPath)
        guard let helperTeam, !helperTeam.isEmpty else {
            return .unsigned(helperPath)
        }
        guard helperTeam == appTeam else {
            return .teamMismatch(helper: helperPath, expected: appTeam, found: helperTeam)
        }
        return nil
    }

    static func readTeamIdentifier(ofBinaryAt path: String) -> String? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/codesign")
        process.arguments = ["-dv", "--verbose=4", path]
        let pipe = Pipe()
        // codesign writes its description to stderr.
        process.standardError = pipe
        process.standardOutput = Pipe()
        do {
            try process.run()
        } catch {
            return nil
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        return teamIdentifier(inCodesignOutput: String(decoding: data, as: UTF8.self))
    }

    /// Parses `TeamIdentifier=...` out of codesign's description.
    ///
    /// `TeamIdentifier=not set` is codesign's spelling for an ad-hoc signature
    /// and must read as absent, not as a team literally named "not set".
    static func teamIdentifier(inCodesignOutput output: String) -> String? {
        for line in output.split(separator: "\n") {
            guard line.hasPrefix("TeamIdentifier=") else { continue }
            let value = String(line.dropFirst("TeamIdentifier=".count))
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return (value.isEmpty || value == "not set") ? nil : value
        }
        return nil
    }
}
