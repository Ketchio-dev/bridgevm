import Foundation

/// Collect only the displayed suffix; a command can contain far more lines than the UI draws.
enum HvfEventFeedLines {
    static func render<Texts: Sequence>(_ newestFirst: Texts, count: Int, length: Int) -> String where Texts.Element == String {
        guard count > 0, length > 0 else { return "" }
        var lines: [String] = []
        for text in newestFirst {
            var end = text.endIndex
            while lines.count < count {
                let rest = text[..<end]
                guard let separator = rest.lastIndex(of: "\n") else {
                    lines.append(truncate(rest, length: length))
                    break
                }
                lines.append(truncate(text[text.index(after: separator)..<end], length: length))
                end = separator
            }
            if lines.count == count { break }
        }
        return lines.reversed().joined(separator: "\n")
    }

    private static func truncate(_ line: Substring, length: Int) -> String {
        let prefix = line.prefix(length + 1)
        return prefix.count > length ? prefix.prefix(length) + "…" : String(prefix)
    }
}
