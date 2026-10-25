import Foundation

/// One runtime session's app-side pacing records, the measurements the exporter's `ramfb display timing:`
/// record cannot make: what the live display window presented (`app display pacing:`,
/// HvfDisplayPacingRecord) and how the ordered input route kept up (`app ordered input:`,
/// HvfOrderedInputRecord).
///
/// HvfEngineSession begins them as it starts or attaches a runtime (`resetObservedRuntimeState`) and ends
/// them once as that runtime stops (`markStopped`). A session that recorded anything then replaces
/// `app-pacing.log` in its evidence directory with the two lines, so the file always holds the last ended
/// session. Display ticks and input outside a session are not counted. Recording moves fixed-size counters
/// on the main thread, logs nothing per frame or event, changes nothing the app shows or sends, and gates
/// nothing. The T17 journey keeps its first run's file (T17FirstRunAppPacing.swift), and
/// scripts/live-gates/app_pacing_diagnostics.py summarises a file or a lane result.
@MainActor
final class HvfAppPacingRecords {
    private(set) var display = HvfDisplayPacingRecord()
    private(set) var input = HvfOrderedInputRecord()
    let stalls = HvfMainThreadStallObserver()
    private(set) var open = false
    var clock: () -> UInt64 = HvfAppPacingClock.now

    func begin() {
        (display, input, open) = (HvfDisplayPacingRecord(), HvfOrderedInputRecord(), true)
        stalls.reset()
    }

    /// One display-link tick (FBLayerView.step); the session's first also starts the stall observer.
    func display(_ tick: HvfDisplayTick, timestamp: Double, target: Double, link: ObjectIdentifier, work: UInt64) {
        guard open else { return }
        if !stalls.running { stalls.start(clock: clock) }
        display.note(tick, timestamp: timestamp, target: target, link: link, work: work)
    }

    /// Counts `driver`'s admissions and stream updates while a session is open.
    func observe(_ driver: HvfSessionInputDriver) {
        driver.onAdmission = { [weak self] admission, held, depth in
            guard let self, self.open else { return }
            self.input.admitted(admission, held: held, depth: depth)
        }
        driver.onUpdate = { [weak self] update in
            guard let self, self.open else { return }
            self.input.note(update, now: self.clock())
        }
    }

    var lines: [String] { [display.line(stalls: stalls.spans), input.line()] }

    /// Ends an open session; true when its records were written to `evidenceDir`.
    @discardableResult
    func end(evidenceDir: String) -> Bool {
        guard open else { return false }
        open = false
        stalls.stop()
        guard display.ticks > 0 || input.recorded else { return false }
        return HvfAppPacingFile.write(lines, evidenceDir: evidenceDir)
    }
}

enum HvfAppPacingClock {
    /// CLOCK_UPTIME_RAW nanoseconds: the timebase of CADisplayLink times (CACurrentMediaTime) and of the
    /// BVFB header's publish time (crates/bridgevm-hvf/src/virtio_gpu/fb_sink_header.rs).
    static func now() -> UInt64 { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }
}

enum HvfAppPacingFile {
    static let name = "app-pacing.log"

    /// Replaces `app-pacing.log` in the existing directory `evidenceDir` with `lines` by an atomic rename,
    /// which replaces a link at that name rather than writing through it; false when it could not.
    static func write(_ lines: [String], evidenceDir: String) -> Bool {
        var directory: ObjCBool = false
        guard !evidenceDir.isEmpty, FileManager.default.fileExists(atPath: evidenceDir, isDirectory: &directory),
              directory.boolValue else { return false }
        let url = URL(fileURLWithPath: evidenceDir, isDirectory: true).appendingPathComponent(name)
        do {
            try Data(lines.map { $0 + "\n" }.joined().utf8).write(to: url, options: .atomic)
            return true
        } catch {
            return false
        }
    }
}
