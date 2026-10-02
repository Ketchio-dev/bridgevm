import Foundation

/// The event feed's on-screen text, bounded so SwiftUI measures a small string. Studio T17 r78
/// sampled a stall as CoreText shaping the whole 500-event feed (hex dumps, command bodies) several
/// times per layout. The session keeps every event; this only limits what is drawn.
enum HvfEventFeedText {
    static let shownEvents = 200
    static let lineLimit = 240

    static func render(_ events: [BvAgentEvent]) -> String {
        events.suffix(shownEvents).flatMap { $0.displayText.split(separator: "\n", omittingEmptySubsequences: false) }
            .map { $0.count > lineLimit ? $0.prefix(lineLimit) + "…" : String($0) }
            .joined(separator: "\n")
    }
}
