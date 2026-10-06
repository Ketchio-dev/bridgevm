import Foundation

/// Collect only the displayed suffix; a command can contain far more lines than the UI draws.
enum HvfEventFeedLines {
    static func render<Texts: Sequence>(_ newestFirst: Texts, count: Int, length: Int) -> String where Texts.Element == String {
        guard count > 0, length > 0 else { return "" }
        var lines: [String] = []
        for text in newestFirst {
            let scalars = text.unicodeScalars
            var end = scalars.endIndex
            while lines.count < count {
                let rest = scalars[..<end]
                guard let separator = rest.lastIndex(where: HvfEventFeedLineBreak.matches) else {
                    lines.append(HvfEventFeedLinePrefix.render(rest, length: length))
                    break
                }
                lines.append(HvfEventFeedLinePrefix.render(scalars[scalars.index(after: separator)..<end], length: length))
                end = HvfEventFeedLineBreak.start(of: separator, in: scalars)
            }
            if lines.count == count { break }
        }
        return lines.reversed().joined(separator: "\n")
    }

}
