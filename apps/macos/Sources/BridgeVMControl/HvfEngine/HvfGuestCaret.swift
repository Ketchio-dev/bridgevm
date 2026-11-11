import Foundation
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// The guest's text caret, so the composing-text overlay and the Mac IME's candidate window sit where the user
/// types in Windows. The agent's `CARET <id>` (bvagent.ps1 Get-BvCaret, advertised by the INPUTCAPS marker
/// `BVINPUT_TEXT <id> CARET`) answers through the usual frame:
/// `BVAGENT CMD CARET <id> exit=0`, then `BVCARET <x> <y> <w> <h>` in virtual-desktop pixels, or `BVCARET none`.
enum HvfGuestCaret {
    /// No desktop side is larger (the agent's DISPLAYMODE takes at most 16384).
    static let maximumCoordinate = 16384

    /// The caret in guest pixels from a reply line. Guest bytes: only four plain decimal numbers of at most five
    /// digits, none above a desktop's side and a nonzero height, count; `none` and anything else are nil.
    static func parse(_ line: String) -> CGRect? {
        let fields = line.split(separator: " ", omittingEmptySubsequences: false)
        guard fields.count == 5, fields[0] == "BVCARET" else { return nil }
        var values: [Int] = []
        for field in fields.dropFirst() {
            guard (1...5).contains(field.utf8.count), field.utf8.allSatisfy({ (UInt8(ascii: "0")...UInt8(ascii: "9")).contains($0) }),
                  let value = Int(field), value <= maximumCoordinate else { return nil }
            values.append(value)
        }
        guard values[3] > 0 else { return nil }
        return CGRect(x: values[0], y: values[1], width: values[2], height: values[3])
    }
}

/// One `CARET <id>` question. Its answer is the CMD frame carrying that ID, so the reply to an earlier question
/// is never taken for it. A failed command, a malformed line, a guest restart or no answer within `timeout` is
/// `.none`: the overlay stays where it is without a caret.
struct HvfGuestCaretRequest {
    enum Answer: Equatable { case caret(CGRect), none }
    static let timeout: TimeInterval = 0.3
    let command: String
    let deadline: Date
    private var answering = false

    init(now: Date, id: UUID = UUID()) {
        command = "CARET " + id.uuidString
        deadline = now.addingTimeInterval(Self.timeout)
    }

    /// nil while unanswered and before the deadline; the lines are read before the deadline is checked.
    mutating func consume(lines: [String], now: Date) -> Answer? {
        for raw in lines {
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            if line.hasPrefix("BVAGENT READY") || line.hasPrefix("BVAGENT re-READY") ||
                line.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches(line) { return Answer.none }
            if answering {
                if line.hasPrefix("BVCARET ") { return HvfGuestCaret.parse(line).map(Answer.caret) ?? Answer.none }
                if line == "BVAGENT END \(command)" { return Answer.none }
            } else if line.hasPrefix("BVAGENT CMD \(command) exit=") {
                guard line == "BVAGENT CMD \(command) exit=0" else { return Answer.none }
                answering = true
            }
        }
        return now < deadline ? nil : Answer.none
    }
}

#if canImport(AppKit)
extension HvfGuestCaret {
    /// `caret` (guest pixels) in the display view's flipped coordinates, mapped as the image is shown
    /// (HvfDisplayFit: aspect fit, letterboxed and centred; the inverse of the pointer's mapping), clipped to the
    /// image. nil when it does not start on the guest image or there is no image.
    static func viewRect(_ caret: CGRect, viewSize: CGSize, imageSize: CGSize) -> CGRect? {
        guard let fit = HvfDisplayFit(viewSize: viewSize, imageSize: imageSize),
              caret.minX >= 0, caret.minY >= 0, caret.minX < imageSize.width, caret.minY < imageSize.height else { return nil }
        let width = min(caret.width, imageSize.width - caret.minX), height = min(caret.height, imageSize.height - caret.minY)
        return CGRect(x: fit.rect.minX + caret.minX * fit.scale, y: fit.rect.minY + caret.minY * fit.scale,
                      width: width * fit.scale, height: height * fit.scale)
    }

    /// Where the overlay's top-left corner goes, as distances from the view's left and top edges, for a caret
    /// given the same way: just below the caret, above it when there is no room below, always inside the margin.
    static func overlayOrigin(caret: CGRect, overlay: CGSize, view: CGSize, margin: CGFloat = 12, gap: CGFloat = 2) -> CGPoint {
        let x = min(max(caret.minX, margin), max(margin, view.width - margin - overlay.width))
        var y = caret.maxY + gap
        if y + overlay.height > view.height - margin { y = caret.minY - gap - overlay.height }
        return CGPoint(x: x, y: min(max(y, margin), max(margin, view.height - margin - overlay.height)))
    }
}
#endif
