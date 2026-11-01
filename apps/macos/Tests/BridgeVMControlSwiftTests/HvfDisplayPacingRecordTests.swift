// sources: apps/macos/Sources/BridgeVMControl/HvfEngine/HvfDisplayPacingRecord.swift apps/macos/Sources/BridgeVMControl/HvfEngine/HvfMainThreadStalls.swift
import Foundation
import Testing

/// The live display's `app display pacing:` record. Times use a 1/64 s frame period, which binary
/// floating point holds exactly, so every expected hold, gap and age below is exact.
@Suite("HvfDisplayPacingRecord")
struct HvfDisplayPacingRecordTests {
    static let period = 1.0 / 64
    final class Link {}
    let link = ObjectIdentifier(Link.self)

    /// The vsync time of tick `k` from t = 100 s.
    static func vsync(_ k: Int) -> Double { 100 + Double(k) * period }
    /// A publish time `before` seconds ahead of tick `k`'s vsync, in CLOCK_UPTIME_RAW nanoseconds.
    static func published(_ k: Int, before: Double) -> UInt64 { UInt64((vsync(k) - before) * 1e9) }

    func tick(_ record: inout HvfDisplayPacingRecord, _ k: Int, _ outcome: HvfDisplayTick,
              link: ObjectIdentifier? = nil, work: UInt64 = 0) {
        record.note(outcome, timestamp: Self.vsync(k), target: Self.vsync(k + 1), link: link ?? self.link, work: work)
    }

    /// Content at half the display rate: a new frame every other tick, each published 1/128 s before
    /// the vsync of the tick that takes it.
    func halfRate(ticks: Int) -> HvfDisplayPacingRecord {
        var record = HvfDisplayPacingRecord()
        for k in 0..<ticks {
            guard k % 2 == 0 else { tick(&record, k, .repeated); continue }
            let sequence = UInt64(k + 2), previous: UInt64 = k == 0 ? .max : UInt64(k)
            tick(&record, k, .presented(sequence: sequence, previous: previous,
                                        published: Self.published(k, before: 1.0 / 128)))
        }
        return record
    }

    /// The sample record scripts/live-gates tests and T17FirstRunAppPacingTests.swift also pin.
    static let sample = "app display pacing: ticks=12 presented=6 repeated=6 busy=0 unmapped=0 iosurface=0 "
        + "first_frames=1 skipped=0 period_min_us=15625 period_max_us=15625 missed_vsyncs=0 max_tick_gap_ms=15 "
        + "hold_1=0 hold_2=5 hold_3=0 hold_4=0 hold_5=0 hold_6=0 hold_gt_6=0 age_le_8ms=0 age_le_17ms=0 "
        + "age_le_33ms=5 age_le_50ms=0 age_le_100ms=0 age_gt_100ms=0 age_unknown=0 max_age_us=23437 "
        + "max_work_us=0 stalls_gt_50ms=0 max_stall_ms=0"

    @Test("even content holds every frame the same number of vsyncs, and the line names every field once")
    func evenHolds() {
        let record = halfRate(ticks: 12)
        #expect(record.line(stalls: HvfMainThreadSpans()) == Self.sample)
        #expect(record.holds == [0, 5, 0, 0, 0, 0, 0])
        #expect(Self.sample.split(separator: " ").dropFirst(3).map { String($0.split(separator: "=")[0]) }
                == HvfDisplayPacingRecord.fields)
    }

    @Test("uneven holds, frames that never reached the layer and the repeated ticks between")
    func unevenHolds() {
        var record = HvfDisplayPacingRecord()
        tick(&record, 0, .presented(sequence: 2, previous: .max, published: Self.published(0, before: 0.00390625)))
        tick(&record, 1, .presented(sequence: 4, previous: 2, published: Self.published(1, before: 0.00390625)))
        tick(&record, 2, .repeated)
        tick(&record, 3, .busy)
        // Three frames were published since sequence 4; the layer gets only the last.
        tick(&record, 4, .presented(sequence: 10, previous: 4, published: Self.published(4, before: 0.00390625)))
        tick(&record, 5, .repeated)
        tick(&record, 6, .presented(sequence: 12, previous: 10, published: Self.published(6, before: 0.00390625)))
        tick(&record, 14, .presented(sequence: 14, previous: 12, published: Self.published(14, before: 0.00390625)))
        #expect(record.holds == [1, 1, 1, 0, 0, 0, 1], "1, 3, 2 and 8 vsyncs")
        #expect([record.ticks, record.presented, record.repeated, record.busy, record.firstFrames] == [8, 5, 2, 1, 1])
        #expect(record.skipped == 2)
        #expect(record.missedVsyncs == 7, "tick 14 came 8 vsyncs after tick 6")
        #expect(record.maxTickGap == 125_000)
        // Published 1/256 s before the tick's vsync and shown one period later: 19.53125 ms old.
        #expect(record.ages == [0, 0, 4, 0, 0, 0] && record.maxAge == 19_531)
    }

    @Test("a new display link or mapping starts a new run without a gap, hold, skip or age")
    func restarts() {
        var record = HvfDisplayPacingRecord()
        tick(&record, 0, .presented(sequence: 2, previous: .max, published: Self.published(0, before: 0.0078125)))
        tick(&record, 1, .presented(sequence: 4, previous: 2, published: Self.published(1, before: 0.0078125)))
        // The window was hidden for a minute; its next link is a new object.
        final class Next {}
        let next = ObjectIdentifier(Next.self)
        tick(&record, 3_840, .presented(sequence: 40, previous: 4, published: Self.published(5, before: 0)), link: next)
        tick(&record, 3_841, .unmapped, link: next)
        // The file was remapped: the reader starts again from UInt64.max.
        tick(&record, 3_842, .presented(sequence: 42, previous: .max,
                                        published: Self.published(3_842, before: 0.0078125)), link: next)
        #expect([record.presented, record.firstFrames, record.unmapped, record.skipped] == [4, 3, 1, 0])
        #expect(record.holds.reduce(0, +) == 1 && record.ages.reduce(0, +) == 1)
        #expect(record.missedVsyncs == 0 && record.maxTickGap == 15_625)
    }

    @Test("an age is unknown without a publish time or with one not before the frame's vsync")
    func ages() {
        var record = HvfDisplayPacingRecord()
        tick(&record, 0, .presented(sequence: 2, previous: .max, published: 0))
        tick(&record, 1, .presented(sequence: 4, previous: 2, published: 0))
        tick(&record, 2, .presented(sequence: 6, previous: 4, published: Self.published(3, before: 0)))
        tick(&record, 3, .presented(sequence: 8, previous: 6, published: Self.published(3, before: 0.109375)))
        tick(&record, 4, .presented(sequence: 10, previous: 8, published: Self.published(4, before: 0)))
        #expect(record.ageUnknown == 2, "no time, and a time at the vsync it shows at")
        #expect(record.ages == [0, 1, 0, 0, 0, 1], "one period, and 109.375 + 15.625 ms")
        #expect(record.maxAge == 125_000)
    }

    @Test("IOSurface, unmapped and untimed ticks only count; the longest tick is kept in microseconds")
    func untimed() {
        var record = HvfDisplayPacingRecord()
        tick(&record, 0, .iosurface, work: 2_500_000)
        tick(&record, 1, .unmapped, work: 9_999)
        record.note(.presented(sequence: 2, previous: 0, published: 1), timestamp: 5, target: 5, link: link, work: 0)
        record.note(.presented(sequence: 4, previous: 2, published: 1), timestamp: .nan, target: 6, link: link, work: 0)
        // No display has a 0.5 ms or 0.5 us period: such ticks neither move the period range nor skip vsyncs,
        // so the record still reconciles (period_min_us is 0 only with period_max_us).
        record.note(.presented(sequence: 6, previous: 4, published: 1), timestamp: 7, target: 7.0005, link: link, work: 0)
        record.note(.repeated, timestamp: 7.001, target: 7.0010005, link: link, work: 0)
        #expect([record.ticks, record.iosurface, record.unmapped, record.presented, record.firstFrames] == [6, 1, 1, 3, 3])
        #expect([record.periodMin, record.periodMax, record.missedVsyncs, record.ageUnknown] == [15_625, 15_625, 0, 0])
        #expect(record.line(stalls: HvfMainThreadSpans()).contains(" max_work_us=2500 "))
    }

    @Test("main-thread stretches leave out sleep, and only those over 50 ms are stalls")
    func stalls() {
        var spans = HvfMainThreadSpans()
        let ms: UInt64 = 1_000_000
        spans.note(sleeping: false, at: 0)
        spans.note(sleeping: false, at: 30 * ms)
        spans.note(sleeping: true, at: 100 * ms)  // 70 ms busy before waiting
        spans.note(sleeping: false, at: 900 * ms) // asleep until here
        spans.note(sleeping: false, at: 950 * ms) // exactly 50 ms: not over
        spans.note(sleeping: true, at: 951 * ms)
        #expect(spans.stalls == 1 && spans.longest == 70 * ms)
        var record = HvfDisplayPacingRecord()
        tick(&record, 0, .repeated)
        #expect(record.line(stalls: spans).hasSuffix(" stalls_gt_50ms=1 max_stall_ms=70"))
    }

    @Test("the stall observer starts once on the main run loop and stops")
    @MainActor func observerLifecycle() {
        let observer = HvfMainThreadStallObserver()
        #expect(!observer.running)
        observer.start(clock: { 0 })
        observer.start(clock: { 1 })
        #expect(observer.running)
        observer.stop()
        #expect(!observer.running)
        observer.reset()
        #expect(observer.spans == HvfMainThreadSpans())
    }

    @Test("vsync counts round to the nearest whole vsync, at least one")
    func vsyncCounts() {
        #expect(HvfDisplayPacingRecord.vsyncs(1.0 / 64, 1.0 / 64) == 1)
        #expect(HvfDisplayPacingRecord.vsyncs(1.4 / 64, 1.0 / 64) == 1)
        #expect(HvfDisplayPacingRecord.vsyncs(1.6 / 64, 1.0 / 64) == 2)
        #expect(HvfDisplayPacingRecord.vsyncs(0.1 / 64, 1.0 / 64) == 1)
        #expect(HvfDisplayPacingRecord.vsyncs(.infinity, 1.0 / 64) == 1)
        #expect(HvfDisplayPacingRecord.vsyncs(1e12, 1e-6) == 1_000_000_000)
        #expect(HvfDisplayPacingRecord.micros(-1) == 0 && HvfDisplayPacingRecord.micros(.nan) == 0)
    }
}
