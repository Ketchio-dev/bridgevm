import Foundation

/// Host-only preedit storage. Guest text is deliberately absent: the display cannot
/// implement replacements in a guest application's document or report its caret.
struct HvfTextCompositionState {
    private(set) var text = ""
    private(set) var selection = NSRange(location: 0, length: 0)
    var range: NSRange { text.isEmpty ? NSRange(location: NSNotFound, length: 0) : NSRange(location: 0, length: text.utf16.count) }

    mutating func mark(_ value: String, selectedRange: NSRange, replacementRange: NSRange) -> Bool {
        guard let updated = replacing(with: value, range: replacementRange),
              let selected = Self.indices(selectedRange, in: value) else { return false }
        let offset = replacementRange.location == NSNotFound ? 0 : replacementRange.location
        text = updated
        selection = NSRange(location: offset + NSRange(selected, in: value).location, length: selectedRange.length)
        return true
    }

    func replacing(with value: String, range: NSRange) -> String? {
        if range.location == NSNotFound { return value }
        guard let indices = Self.indices(range, in: text) else { return nil }
        return text.replacingCharacters(in: indices, with: value)
    }

    mutating func clear() { self = Self() }

    static func indices(_ range: NSRange, in text: String) -> Range<String.Index>? {
        let units = Array(text.utf16)
        guard range.location >= 0, range.length >= 0, range.location <= units.count,
              range.length <= units.count - range.location else { return nil }
        for offset in [range.location, range.location + range.length] where offset > 0 && offset < units.count {
            if (0xd800...0xdbff).contains(units[offset - 1]), (0xdc00...0xdfff).contains(units[offset]) { return nil }
        }
        return Range(range, in: text)
    }
}
