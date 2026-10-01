import Foundation

/// One row of the runtime event feed. Its id is the event's position in the session's whole
/// history, so trimming the 500-event window never re-identifies the rows that remain. With
/// offset ids every append past the cap shifted every row's content, and SwiftUI redid layout
/// and accessibility for the whole feed on each poll (Studio T17 r73: the main thread spent
/// 72 s at 100% user CPU while every non-BVAGENT run-log line became an event).
struct HvfEventFeedRow: Identifiable {
    let id: Int
    let event: BvAgentEvent
}

extension HvfEngineSession {
    var eventFeed: [HvfEventFeedRow] {
        events.enumerated().map { HvfEventFeedRow(id: eventBase + $0.offset, event: $0.element) }
    }
}
