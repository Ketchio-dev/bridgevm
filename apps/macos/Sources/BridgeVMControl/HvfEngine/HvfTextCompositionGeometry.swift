#if canImport(AppKit)
import AppKit

enum HvfTextCompositionGeometry {
    /// Coordinates belong to the host's single-line preedit, never a guest caret.
    static func rect(text: String, requested: NSRange, anchor: NSRect, font: NSFont) -> (rect: NSRect, range: NSRange) {
        let missing = (NSRect.zero, NSRange(location: NSNotFound, length: 0))
        guard requested.location != NSNotFound, requested.location >= 0, requested.length >= 0,
              requested.location <= text.utf16.count else { return missing }
        let range = NSRange(location: requested.location, length: min(requested.length, text.utf16.count - requested.location))
        guard let selected = HvfTextCompositionState.indices(range, in: text),
              let prefix = HvfTextCompositionState.indices(NSRange(location: 0, length: range.location), in: text) else { return missing }
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        let x = min(anchor.maxX, anchor.minX + (String(text[prefix]) as NSString).size(withAttributes: attributes).width)
        let width = range.length == 0 ? 0 : min(anchor.maxX - x,
            (String(text[selected]) as NSString).size(withAttributes: attributes).width)
        return (NSRect(x: x, y: anchor.minY, width: max(0, width), height: anchor.height), range)
    }
}
#endif
