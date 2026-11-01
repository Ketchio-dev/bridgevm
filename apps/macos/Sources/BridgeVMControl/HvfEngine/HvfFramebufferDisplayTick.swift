#if canImport(AppKit)
import AppKit
import QuartzCore

extension FBLayerView {
    /// One display-link tick: show the newest frame, then count the tick in the runtime session's
    /// `app display pacing:` record (HvfAppPacingRecords.swift). Counting reads only the link's times,
    /// the tick's outcome and its duration; it changes nothing the view shows or requests.
    @objc func step(_ link: CADisplayLink) {
        let started = HvfAppPacingClock.now()
        let tick = refreshFrame(at: link.timestamp)
        session?.appPacing.display(tick, timestamp: link.timestamp, target: link.targetTimestamp,
                                   link: ObjectIdentifier(link), work: HvfAppPacingClock.now() &- started)
    }
}

extension HvfDisplayTick {
    /// A tick whose read showed nothing new.
    init(unpresented read: HvfFramebufferFrameReader.Outcome) {
        switch read {
        case .unchanged: self = .repeated
        case .invalid: self = .unmapped
        case .busy, .frame: self = .busy
        }
    }
}
#endif
