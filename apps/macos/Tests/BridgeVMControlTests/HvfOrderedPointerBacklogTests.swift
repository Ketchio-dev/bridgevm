import Foundation
import XCTest
@testable import BridgeVMControl

/// Continuous motion on the ordered route, scripted after the 2026-10-22 display investigation's
/// real-router bench: 10 s of one absolute move per display tick, as `HvfPointerMoveMailbox` delivers
/// them. The guest agent's receipt is visible `rtt` after each send. Polls follow
/// `HvfSessionInputDriver`: one after every queued route, a 10 ms pump while the router holds input,
/// and a poll without lines after an inserted receipt. A move's lag is its send time plus `rtt`
/// minus the time that position was routed.
final class HvfOrderedPointerBacklogTests: XCTestCase {
    func testSixtyHertzMotionWithFiftyMillisecondReceiptsIsNeitherRefusedNorDelayed() {
        assertCurrentPositionIsSent(ScriptedMotion(hz: 60, rttMs: 50).run())
    }

    func testOneHundredTwentyHertzMotionWithTwentyMillisecondReceiptsIsNeitherRefusedNorDelayed() {
        assertCurrentPositionIsSent(ScriptedMotion(hz: 120, rttMs: 20).run())
    }

    private func assertCurrentPositionIsSent(_ run: ScriptedMotion.Result) {
        print(run.summary)
        XCTAssertFalse(run.failed, run.summary)
        XCTAssertEqual(run.routed, run.hz * 10, run.summary)
        XCTAssertEqual(run.refused, 0, run.summary)
        XCTAssertEqual(run.legacy, 0, run.summary)
        XCTAssertGreaterThanOrEqual(2, run.maxQueue, "queue depth: \(run.summary)")
        XCTAssertGreaterThanOrEqual(run.rttNs + run.periodNs, run.lagsNs.max() ?? .max,
                                    "every sent move within one round trip and one display period: \(run.summary)")
        XCTAssertEqual(run.lastSent, run.lastRouted, "the final position is sent: \(run.summary)")
    }
}

private struct ScriptedMotion {
    let hz: Int
    let rttMs: Int

    struct Result {
        let hz: Int, rttNs: Int, periodNs: Int
        var routed = 0, refused = 0, legacy = 0, maxQueue = 0, motionSends = 0
        var lagsNs: [Int] = [], motionLagsNs: [Int] = []
        var lastRouted = "", lastSent = ""
        var failed = false

        var summary: String {
            func ms(_ ns: Int) -> String { String(format: "%.1f", Double(ns) / 1_000_000) }
            func percentile(_ values: [Int], _ p: Double) -> Int {
                guard !values.isEmpty else { return 0 }
                let sorted = values.sorted()
                return sorted[Int((p / 100 * Double(sorted.count - 1)).rounded())]
            }
            return "ordered hz=\(hz) rtt_ms=\(rttNs / 1_000_000) routed=\(routed) refused=\(refused) legacy=\(legacy)"
                + " max_queue=\(maxQueue) sent=\(lagsNs.count) sent_during_motion=\(motionSends)"
                + " lag_ms_during_motion p50=\(ms(percentile(motionLagsNs, 50))) p95=\(ms(percentile(motionLagsNs, 95)))"
                + " max=\(ms(motionLagsNs.max() ?? 0)) lag_ms_all_max=\(ms(lagsNs.max() ?? 0))"
                + " last_routed=\(lastRouted) last_sent=\(lastSent) failed=\(failed)"
        }
    }

    func run() -> Result {
        let binding = ["disk", "vars", "evidence", "control"]
        let second = 1_000_000_000, rtt = rttMs * 1_000_000, pump = 10_000_000, ticks = hz * 10
        var result = Result(hz: hz, rttNs: rtt, periodNs: (second + hz - 1) / hz)
        var router = HvfSessionInputRouter()
        router.beginOwnedBoot(binding: binding)
        var now = 0, motionEnd = Int.max
        var pumpAt: Int?
        var receipts: [(visible: Int, lines: [String])] = []
        var routedAt: [String: Int] = [:]
        func date(_ ns: Int) -> Date { Date(timeIntervalSinceReferenceDate: 1_000_000 + Double(ns) / Double(second)) }
        func send(_ command: String) -> Bool {
            let fields = command.split(separator: " ").map(String.init)
            guard fields.count >= 2 else { return false }
            if fields[0] == "INPUTCAPS" {
                receipts.append((now + pump, ["BVAGENT CMD \(command) exit=0",
                    "BVINPUT_CAPS \(fields[1]) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536", "BVAGENT END \(command)"]))
                return true
            }
            if fields.count == 3, let bytes = Data(base64Encoded: fields[2]),
               let routed = routedAt[String(decoding: bytes, as: UTF8.self)] {
                result.lastSent = String(decoding: bytes, as: UTF8.self)
                result.lagsNs.append(now + rtt - routed)
                if now < motionEnd { result.motionSends += 1; result.motionLagsNs.append(now + rtt - routed) }
            }
            let label = "\(fields[0]) \(fields[1])"
            receipts.append((now + rtt, ["BVAGENT CMD \(label) exit=0", "BVINPUT_INSERTED \(fields[1]) 1", "BVAGENT END \(label)"]))
            return true
        }
        func poll() {
            let due = receipts.filter { $0.visible <= now }.flatMap(\.lines)
            receipts.removeAll { $0.visible <= now }
            if case .inserted = router.pollBatch(binding: binding, serviceReady: true, lines: due, now: date(now), send: send) {
                _ = router.poll(binding: binding, serviceReady: true, lines: [], now: date(now), send: send)
            }
            if router.count == 0 { pumpAt = nil }
            result.maxQueue = max(result.maxQueue, router.count)
        }
        poll()
        while router.state != .ready, now < second {
            now += 5_000_000
            poll()
        }
        guard router.state == .ready else { result.failed = true; return result }
        let begin = now
        motionEnd = begin + ticks * second / hz
        var tick = 0
        while true {
            let nextTick = tick < ticks ? begin + tick * second / hz : Int.max
            now = min(nextTick, pumpAt ?? Int.max)
            if now == Int.max { break }
            guard now < begin + 60 * second else { result.failed = true; break }
            if now == nextTick {
                tick += 1
                let move = "move:\(100 + 10 * tick)x300"
                result.routed += 1
                switch router.route(.pointer(move), binding: binding, now: date(now)) {
                case .queued:
                    routedAt[move] = now
                    result.lastRouted = move
                    if router.count > 0, pumpAt == nil { pumpAt = now + pump }
                    poll()
                case .refused: result.refused += 1
                case .legacy: result.legacy += 1
                }
            }
            if let at = pumpAt, now == at {
                pumpAt = at + pump
                poll()
            }
        }
        result.failed = result.failed || router.failed
        return result
    }
}
