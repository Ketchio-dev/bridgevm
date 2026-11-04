import Foundation

/// Windows-safe names for dropped files. The guest agent refuses any other name (bvagent.ps1
/// `Test-DropName`), so staging repairs them: NFC (Finder often stores Hangul decomposed), `_` for
/// characters Windows forbids, no trailing dots or spaces, and `_` before a reserved device name.
enum HvfFileDropNames {
    private static let forbidden = CharacterSet(charactersIn: "<>:\"/\\|?*")
        .union(CharacterSet(charactersIn: Unicode.Scalar(0)...Unicode.Scalar(0x1F)))
    private static let reserved: Set<String> = Set(["CON", "PRN", "AUX", "NUL"]
        + ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].flatMap { ["COM\($0)", "LPT\($0)"] })

    static func windowsName(_ name: String) -> String {
        var scalars = String.UnicodeScalarView()
        for scalar in name.precomposedStringWithCanonicalMapping.unicodeScalars {
            scalars.append(forbidden.contains(scalar) ? "_" : scalar)
        }
        var cleaned = String(scalars)
        while let last = cleaned.last, last == "." || last == " " { cleaned.removeLast() }
        if cleaned.isEmpty { return "_" }
        // Windows matches the part before the first dot, ignoring trailing spaces: "con .txt" is CON.
        var stem = Substring(cleaned.split(separator: ".", omittingEmptySubsequences: false).first ?? "")
        while stem.last == " " { stem.removeLast() }
        return reserved.contains(stem.uppercased()) ? "_" + cleaned : cleaned
    }

    /// `name`, else `name (2).ext`, `name (3).ext`... (`name (2)` for a folder): the first that
    /// `taken` does not already hold, ignoring case as Windows does.
    static func unique(_ name: String, isDirectory: Bool, taken: inout Set<String>) -> String {
        let dot = isDirectory ? nil : name.lastIndex(of: ".").flatMap { $0 == name.startIndex ? nil : $0 }
        let stem = dot.map { String(name[..<$0]) } ?? name
        let ext = dot.map { String(name[$0...]) } ?? ""
        var candidate = name
        var number = 2
        while taken.contains(candidate.lowercased()) {
            candidate = "\(stem) (\(number))\(ext)"
            number += 1
        }
        taken.insert(candidate.lowercased())
        return candidate
    }
}
