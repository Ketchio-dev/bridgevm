import Foundation

/// The ordered input route's `app ordered input:` record for one runtime session
/// (HvfAppPacingRecords.swift), from HvfSessionInputDriver's admissions and stream updates:
/// - admissions: queued (a coalesced pointer move included), legacy and refused events.
/// - the queue depth after each queued event, the awaited event included; a queued event that left the
///   depth unchanged replaced the newest unsent pointer move and is also `coalesced`.
/// - events sent, events whose inserted receipt answered their send, and events cancellations discarded.
/// - the round trip of each inserted event, from the update that sent it to the one that read its
///   receipt, in whole milliseconds. Both are seen from the app's polls (every 10 ms while events are
///   queued), so a round trip includes the agent's work and the poll delay.
///
/// Buckets take a value at most their bound. Recording never changes what is routed or sent.
struct HvfOrderedInputRecord {
    static let prefix = "app ordered input:"
    static let fields = [
        "queued", "coalesced", "legacy", "refused", "sent", "inserted", "discarded",
        "depth_1", "depth_2", "depth_le_4", "depth_le_16", "depth_gt_16", "max_depth",
        "rtt_le_10ms", "rtt_le_20ms", "rtt_le_50ms", "rtt_le_100ms", "rtt_le_250ms", "rtt_gt_250ms", "max_rtt_ms",
    ]
    static let depthBounds: [UInt64] = [1, 2, 4, 16]
    /// Upper bounds of the round-trip buckets in milliseconds; a longer one is rtt_gt_250ms.
    static let rttBounds: [UInt64] = [10, 20, 50, 100, 250]

    private(set) var queued: UInt64 = 0, coalesced: UInt64 = 0, legacy: UInt64 = 0, refused: UInt64 = 0
    private(set) var sent: UInt64 = 0, inserted: UInt64 = 0, discarded: UInt64 = 0
    private(set) var depths = [UInt64](repeating: 0, count: depthBounds.count + 1)
    private(set) var rtts = [UInt64](repeating: 0, count: rttBounds.count + 1)
    private(set) var maxDepth: UInt64 = 0, maxRtt: UInt64 = 0
    private var awaited: (id: UUID, at: UInt64)?

    var recorded: Bool { queued > 0 || legacy > 0 || refused > 0 || sent > 0 }

    /// One routed event: its admission, with the queue depth before (`held`) and after it.
    mutating func admitted(_ admission: HvfNegotiatedInputStream.Admission, held: Int, depth: Int) {
        switch admission {
        case .legacy: legacy &+= 1
        case .refused: refused &+= 1
        case .queued:
            let after = UInt64(max(depth, 0))
            queued &+= 1
            if depth == held { coalesced &+= 1 }
            depths[Self.depthBounds.firstIndex { after <= $0 } ?? Self.depthBounds.count] &+= 1
            maxDepth = max(maxDepth, after)
        }
    }

    /// One stream update at `now` (nanoseconds).
    mutating func note(_ update: HvfAcknowledgedInputStream.Update?, now: UInt64) {
        switch update {
        case let .sent(id)?:
            sent &+= 1
            awaited = (id, now)
        case let .inserted(id)?:
            if let awaited, awaited.id == id, now >= awaited.at {
                let rtt = (now - awaited.at) / 1_000_000
                inserted &+= 1
                rtts[Self.rttBounds.firstIndex { rtt <= $0 } ?? Self.rttBounds.count] &+= 1
                maxRtt = max(maxRtt, rtt)
            }
            awaited = nil
        case let .cancelled(_, count)?:
            discarded &+= UInt64(max(count, 0))
            awaited = nil
        case .idle?, .waiting?, nil:
            break
        }
    }

    func line() -> String {
        let values = [queued, coalesced, legacy, refused, sent, inserted, discarded] + depths + [maxDepth] + rtts + [maxRtt]
        return Self.prefix + zip(Self.fields, values).map { " \($0)=\($1)" }.joined()
    }
}
