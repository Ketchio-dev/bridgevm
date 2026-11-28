#if canImport(AppKit)
import Foundation
import IOSurface

struct HvfIOSurfaceDescriptorPoller {
    static let interval: TimeInterval = 0.1
    /// No sidecar (the 2D display, the default): looked for about once a second, so 3D shows within one.
    static let missingInterval: TimeInterval = 1
    private var nextCheck: TimeInterval = 0

    mutating func reloadDue(at now: TimeInterval) -> Bool {
        guard now >= nextCheck else { return false }
        nextCheck = now + Self.interval
        return true
    }

    mutating func sidecarMissing(at now: TimeInterval) {
        nextCheck = now + Self.missingInterval
    }

    mutating func reset() {
        nextCheck = 0
    }
}

struct HvfIOSurfacePresentation {
    let surface: IOSurfaceRef
    let width: Int
    let height: Int
    let changed: Bool
}

struct HvfIOSurfacePresenter {
    private var poller = HvfIOSurfaceDescriptorPoller()
    private var descriptor: HvfIOSurfaceDescriptor?
    private var surface: IOSurfaceRef?

    mutating func presentation(
        from url: URL,
        at now: TimeInterval
    ) -> HvfIOSurfacePresentation? {
        var changed = false
        if poller.reloadDue(at: now) {
            let loaded = HvfIOSurfaceDescriptor.load(from: url)
            // A shown sidecar that reads empty keeps the 10 Hz pace: the VMM rewrites it in place.
            if loaded == nil, descriptor == nil { poller.sidecarMissing(at: now) }
            if loaded != descriptor || surface == nil {
                descriptor = loaded
                surface = loaded.flatMap(HvfIOSurfaceScanout.lookup)?.surface
                changed = true
            }
        }
        guard let descriptor, let surface else { return nil }
        return HvfIOSurfacePresentation(
            surface: surface,
            width: descriptor.width,
            height: descriptor.height,
            changed: changed
        )
    }

    mutating func reset() {
        poller.reset()
        descriptor = nil
        surface = nil
    }
}
#endif
