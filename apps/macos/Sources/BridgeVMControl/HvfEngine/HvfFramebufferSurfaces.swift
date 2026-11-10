#if canImport(AppKit)
import Darwin
import IOSurface

/// The 2D display's frames in two IOSurfaces of the frame's size, which the view shows in turn as its
/// layer's contents, so Core Animation composites them without another copy and a frame is never written
/// into the surface on screen. A new frame goes into the surface behind: only what changed since the frame
/// that surface holds when the writer's damage is trusted (HvfFramebufferDamage), else the whole frame.
struct HvfFramebufferSurfaces: HvfFramebufferFrameDestination {
    /// Makes a surface for width x height pixels; nil when IOSurface cannot.
    var make: (Int, Int) -> IOSurfaceRef? = HvfFramebufferSurfaces.surface
    /// A surface could not be made or locked: the view shows CGImages instead.
    private(set) var failed = false
    /// The pixels the newest copy wrote.
    private(set) var copied: HvfFramebufferDamage?

    private var surfaces: [IOSurfaceRef] = []
    private var width = 0, height = 0
    /// The sequence of the frame each surface holds whole; nil while it holds none.
    private var holds: [UInt64?] = [nil, nil]
    private var front = 0
    /// The trusted damage of the frame in front: what the surface behind lacks besides the next frame's.
    private var frontDamage: (sequence: UInt64, rect: HvfFramebufferDamage)?
    /// The newest copy's frame, until the reader's seqlock check lets it be shown.
    private var pending: (sequence: UInt64, damage: HvfFramebufferDamage?)?

    private var back: Int { 1 - front }

    mutating func copy(_ frame: HvfFramebufferFrame) -> Bool {
        if surfaces.isEmpty || frame.width != width || frame.height != height {
            guard let first = make(frame.width, frame.height), let second = make(frame.width, frame.height) else {
                failed = true
                return false
            }
            (surfaces, width, height, holds, frontDamage) = ([first, second], frame.width, frame.height, [nil, nil], nil)
        }
        let region = region(for: frame)
        holds[back] = nil
        guard Self.copy(region, of: frame, into: surfaces[back]) else {
            failed = true
            return false
        }
        (pending, copied) = ((frame.sequence, frame.damage), region)
        return true
    }

    /// A torn copy leaves the surface behind holding no whole frame, so the next copy into it is whole.
    mutating func discard() {
        pending = nil
    }

    /// Brings the surface the newest copy wrote to the front, for the layer to show.
    mutating func present() -> IOSurfaceRef? {
        guard let pending else { return nil }
        self.pending = nil
        holds[back] = pending.sequence
        frontDamage = pending.damage.map { (sequence: pending.sequence, rect: $0) }
        front = back
        return surfaces[front]
    }

    /// Lets the surfaces go.
    mutating func reset() {
        let make = make
        self = Self()
        self.make = make
    }

    /// The surface behind holds the frame two before this one: it lacks the front frame's damage and this
    /// frame's. Anything else is copied whole.
    private func region(for frame: HvfFramebufferFrame) -> HvfFramebufferDamage {
        if let damage = frame.damage, let frontDamage, holds[back] == frame.sequence &- 4,
           frontDamage.sequence == frame.sequence &- 2 {
            return frontDamage.rect.union(damage)
        }
        return .whole(width: frame.width, height: frame.height)
    }

    /// Copies `region` of `frame` under the surface's lock, every pixel opaque: the CGImage this replaces
    /// ignored the fourth byte (noneSkipFirst), which an IOSurface's BGRA format makes alpha.
    private static func copy(_ region: HvfFramebufferDamage, of frame: HvfFramebufferFrame,
                             into surface: IOSurfaceRef) -> Bool {
        guard !region.isEmpty else { return true }
        guard IOSurfaceLock(surface, [], nil) == KERN_SUCCESS else { return false }
        defer { IOSurfaceUnlock(surface, [], nil) }
        let base = IOSurfaceGetBaseAddress(surface), rowBytes = IOSurfaceGetBytesPerRow(surface)
        for row in region.y ..< region.y + region.height {
            let source = frame.pixels.advanced(by: row * frame.stride + region.x * 4)
                .assumingMemoryBound(to: UInt32.self)
            let target = base.advanced(by: row * rowBytes + region.x * 4).assumingMemoryBound(to: UInt32.self)
            for column in 0 ..< region.width {
                target[column] = source[column] | 0xff00_0000
            }
        }
        return true
    }

    /// BGRA, the byte order of the CGImage this replaces (byteOrder32Little | noneSkipFirst).
    static func surface(width: Int, height: Int) -> IOSurfaceRef? {
        IOSurfaceCreate([
            kIOSurfaceWidth as String: width,
            kIOSurfaceHeight as String: height,
            kIOSurfaceBytesPerElement as String: 4,
            kIOSurfacePixelFormat as String: 0x4247_5241, // 'BGRA'
        ] as CFDictionary)
    }
}
#endif
