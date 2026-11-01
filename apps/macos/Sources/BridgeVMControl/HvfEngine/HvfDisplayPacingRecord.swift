import Foundation

/// What one display-link tick of the live display did (FBLayerView.refreshFrame).
enum HvfDisplayTick: Equatable {
    /// The 3D path's IOSurface went to the layer; the BVFB counters do not apply.
    case iosurface
    /// No BVFB file was mapped, or the mapped file stopped being one the view can read.
    case unmapped
    /// The shown frame is still the newest the writer published.
    case repeated
    /// A newer frame could not be taken this tick (the writer held it past the read budget, or no
    /// memory or image), so the shown frame stayed.
    case busy
    /// A new frame went to the layer. `previous` is the sequence shown before it, UInt64.max after
    /// the mapping was (re)made; `published` is its BVFB publish time, 0 when the writer left none.
    case presented(sequence: UInt64, previous: UInt64, published: UInt64)
}

/// The live display's `app display pacing:` record for one runtime session (HvfAppPacingRecords.swift).
///
/// Each display-link tick moves fixed-size counters on the main thread; nothing is logged per frame:
/// - ticks by outcome; they always sum to `ticks`.
/// - the display's frame period (CADisplayLink targetTimestamp - timestamp) range in microseconds, and
///   the vsyncs the link skipped between consecutive ticks with its longest gap. A tick whose period is
///   outside 1 ms to 1 s, which no display has, is untimed: it counts only by its outcome.
/// - each new frame's hold: the whole vsyncs since the previous new frame, 1 to 6 or longer. A frame
///   that starts a run (the first after the mapping or the display link started, or after an untimed new
///   frame) or is untimed has no hold, no age and no skip count; `first_frames` counts those.
/// - frames the writer published that never reached the layer (BVFB sequence gaps between new frames).
/// - each new frame's content age: from its BVFB publish time to its target vsync (not actual presentation), on the shared
///   CLOCK_UPTIME_RAW timebase; unknown when the writer left no time or it is not before that vsync.
/// - the longest tick (the app's own copy, image and layer work) and, from HvfMainThreadStalls, the main
///   run loop's busy stretches over 50 ms.
///
/// Buckets take a value at most their bound, in the unit the record prints. Counting reads only the
/// link's times and the tick's outcome; it changes nothing the view shows or asks the writer for.
struct HvfDisplayPacingRecord: Equatable {
    static let prefix = "app display pacing:"
    static let fields = [
        "ticks", "presented", "repeated", "busy", "unmapped", "iosurface", "first_frames", "skipped",
        "period_min_us", "period_max_us", "missed_vsyncs", "max_tick_gap_ms",
        "hold_1", "hold_2", "hold_3", "hold_4", "hold_5", "hold_6", "hold_gt_6",
        "age_le_8ms", "age_le_17ms", "age_le_33ms", "age_le_50ms", "age_le_100ms", "age_gt_100ms",
        "age_unknown", "max_age_us", "max_work_us", "stalls_gt_50ms", "max_stall_ms",
    ]
    /// Upper bounds of the content-age buckets in microseconds; an older frame is age_gt_100ms.
    static let ageBounds: [UInt64] = [8_000, 17_000, 33_000, 50_000, 100_000]
    /// Holds of 1 to `longestHold` vsyncs have their own bucket; a longer one is hold_gt_6.
    static let longestHold = 6

    private(set) var ticks: UInt64 = 0, presented: UInt64 = 0, repeated: UInt64 = 0, busy: UInt64 = 0
    private(set) var unmapped: UInt64 = 0, iosurface: UInt64 = 0, firstFrames: UInt64 = 0, skipped: UInt64 = 0
    private(set) var periodMin: UInt64 = 0, periodMax: UInt64 = 0, missedVsyncs: UInt64 = 0, maxTickGap: UInt64 = 0
    private(set) var holds = [UInt64](repeating: 0, count: longestHold + 1)
    private(set) var ages = [UInt64](repeating: 0, count: ageBounds.count + 1)
    private(set) var ageUnknown: UInt64 = 0, maxAge: UInt64 = 0, maxWork: UInt64 = 0
    private var link: ObjectIdentifier?
    private var lastTick: Double?, lastPresent: Double?

    /// One tick of display link `link` at vsync `timestamp`, whose content shows at `target` (seconds on
    /// the CLOCK_UPTIME_RAW timebase); `work` is the tick's own duration in nanoseconds.
    mutating func note(_ tick: HvfDisplayTick, timestamp: Double, target: Double, link: ObjectIdentifier, work: UInt64) {
        ticks &+= 1
        maxWork = max(maxWork, work)
        if link != self.link { (self.link, lastTick, lastPresent) = (link, nil, nil) }
        let period = target - timestamp, micros = Self.micros(period)
        let timed = timestamp.isFinite && target.isFinite && micros >= 1000 && period < 1
        if timed {
            (periodMin, periodMax) = (periodMin == 0 ? micros : min(periodMin, micros), max(periodMax, micros))
            if let lastTick, timestamp > lastTick {
                maxTickGap = max(maxTickGap, Self.micros(timestamp - lastTick))
                missedVsyncs &+= Self.vsyncs(timestamp - lastTick, period) - 1
            }
        }
        lastTick = timed ? timestamp : nil
        switch tick {
        case .iosurface: iosurface &+= 1
        case .unmapped: unmapped &+= 1
        case .repeated: repeated &+= 1
        case .busy: busy &+= 1
        case let .presented(sequence, previous, published):
            presented &+= 1
            defer { lastPresent = timed ? timestamp : nil }
            guard timed, previous != .max, let lastPresent, timestamp > lastPresent else {
                firstFrames &+= 1
                return
            }
            holds[Int(min(Self.vsyncs(timestamp - lastPresent, period), UInt64(Self.longestHold + 1))) - 1] &+= 1
            if sequence > previous, (sequence - previous) % 2 == 0 { skipped &+= (sequence - previous) / 2 - 1 }
            noteAge(published: published, target: target)
        }
    }

    func line(stalls: HvfMainThreadSpans) -> String {
        let values = [ticks, presented, repeated, busy, unmapped, iosurface, firstFrames, skipped,
                      periodMin, periodMax, missedVsyncs, maxTickGap / 1000] + holds + ages
            + [ageUnknown, maxAge, maxWork / 1000, stalls.stalls, stalls.longest / 1_000_000]
        return Self.prefix + zip(Self.fields, values).map { " \($0)=\($1)" }.joined()
    }

    private mutating func noteAge(published: UInt64, target: Double) {
        let nanos = (target * 1e9).rounded(.down)
        guard published > 0, nanos.isFinite, nanos >= 0, nanos < 1.8e19, UInt64(nanos) > published else {
            ageUnknown &+= 1
            return
        }
        let age = (UInt64(nanos) - published) / 1000
        ages[Self.ageBounds.firstIndex { age <= $0 } ?? Self.ageBounds.count] &+= 1
        maxAge = max(maxAge, age)
    }

    /// Whole vsyncs in `interval` at `period`, rounded, at least 1.
    static func vsyncs(_ interval: Double, _ period: Double) -> UInt64 {
        let ratio = (interval / period).rounded()
        guard ratio.isFinite, ratio > 1 else { return 1 }
        return ratio >= 1e9 ? 1_000_000_000 : UInt64(ratio)
    }

    static func micros(_ seconds: Double) -> UInt64 {
        let value = (seconds * 1e6).rounded(.down)
        guard value.isFinite, value > 0 else { return 0 }
        return value >= 1e15 ? 1_000_000_000_000_000 : UInt64(value)
    }
}
