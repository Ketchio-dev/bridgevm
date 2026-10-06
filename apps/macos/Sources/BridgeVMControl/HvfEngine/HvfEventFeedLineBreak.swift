import Foundation

/// Normalize Unicode hard line breaks for display; a CRLF pair is one boundary.
enum HvfEventFeedLineBreak {
    static func matches(_ scalar: UnicodeScalar) -> Bool {
        switch scalar.value {
        case 0xA...0xD, 0x85, 0x2028, 0x2029: return true
        default: return false
        }
    }

    static func start(of index: String.UnicodeScalarView.Index, in scalars: String.UnicodeScalarView) -> String.UnicodeScalarView.Index {
        guard scalars[index] == "\n", index > scalars.startIndex else { return index }
        let previous = scalars.index(before: index)
        return scalars[previous] == "\r" ? previous : index
    }
}
