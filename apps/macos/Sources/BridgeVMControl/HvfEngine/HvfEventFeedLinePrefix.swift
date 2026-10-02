import Foundation

/// Bound scalar work before grapheme segmentation: one Character can contain millions of scalars.
enum HvfEventFeedLinePrefix {
    static let scalarLimit = 1_920

    static func render<Scalars: Collection>(_ line: Scalars, length: Int) -> String where Scalars.Element == UnicodeScalar {
        guard length > 0 else { return "" }
        var scalars = String.UnicodeScalarView()
        var scalarCount = 0
        var clipped = false
        for scalar in line.prefix(scalarLimit + 1) {
            if scalarCount == scalarLimit {
                clipped = true
                break
            }
            scalars.append(scalar)
            scalarCount += 1
        }
        let bounded = String(scalars)
        let prefix = bounded.prefix(length + 1)
        if prefix.count > length { return String(prefix.prefix(length)) + "…" }
        return String(prefix) + (clipped ? "…" : "")
    }
}
