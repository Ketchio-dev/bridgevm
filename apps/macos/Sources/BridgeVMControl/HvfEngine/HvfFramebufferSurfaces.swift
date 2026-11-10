#if canImport(AppKit)
import Darwin
import IOSurface

/// The 2D display's frames in a few IOSurfaces of the frame's size, which the view shows as its layer's
/// contents, so Core Animation composites them without another copy. A new frame goes into a surface that is
/// neither shown nor still read by WindowServer (a surface that stopped being the contents can be until it
/// releases it): only what changed since the frame that surface holds when the writer's damage is trusted
/// (HvfFramebufferDamage), else the whole frame. With no such surface free, the frame waits for a later tick.
struct HvfFramebufferSurfaces: HvfFramebufferFrameDestination {
    /// Makes a surface for width x height pixels; nil when IOSurface cannot.
    var make: (Int, Int) -> IOSurfaceRef? = HvfFramebufferSurfaces.surface
    /// Whether another process (WindowServer) still uses a surface.
    var inUse: (IOSurfaceRef) -> Bool = { IOSurfaceIsInUse($0) }
    /// A surface could not be made or locked: the view shows CGImages instead.
    private(set) var failed = false
    /// The pixels the newest copy wrote.
    private(set) var copied: HvfFramebufferDamage?

    /// The surface shown, one WindowServer may not have released yet, and one to write.
    static let limit = 3
    private var surfaces: [IOSurfaceRef] = []
    private var width = 0, height = 0
    /// The sequence of the frame each surface holds whole; nil while it holds none.
    private var holds: [UInt64?] = []
    private var front: Int?
    /// The trusted damage of the frames shown since the last untrusted one, by sequence: what each changed
    /// since the frame before it.
    private var damages: [UInt64: HvfFramebufferDamage] = [:]
    /// The newest copy, until the reader's seqlock check lets it be shown.
    private var pending: (index: Int, sequence: UInt64, damage: HvfFramebufferDamage?)?

    mutating func copy(_ frame: HvfFramebufferFrame) -> Bool {
        if frame.width != width || frame.height != height {
            (surfaces, holds, front, damages, width, height) = ([], [], nil, [:], frame.width, frame.height)
        }
        guard let index = writable() else { return false }
        let region = region(for: frame, holding: holds[index])
        holds[index] = nil
        guard Self.copy(region, of: frame, into: surfaces[index]) else {
            failed = true
            return false
        }
        (pending, copied) = ((index, frame.sequence, frame.damage), region)
        return true
    }

    /// A torn copy leaves its surface holding no whole frame, so the next copy into it is whole.
    mutating func discard() {
        pending = nil
    }

    /// Brings the surface the newest copy wrote to the front, for the layer to show.
    mutating func present() -> IOSurfaceRef? {
        guard let pending else { return nil }
        self.pending = nil
        if let damage = pending.damage {
            damages[pending.sequence] = damage
            damages = damages.filter { pending.sequence &- $0.key < UInt64(2 * Self.limit) }
        } else {
            // Untrusted damage (a first or skipped frame, another file): the other surfaces' frames may not
            // be this file's, and no damage leads from them to this frame.
            holds = holds.map { _ in nil }
            damages = [:]
        }
        holds[pending.index] = pending.sequence
        front = pending.index
        return surfaces[pending.index]
    }

    /// Lets the surfaces go.
    mutating func reset() {
        let (make, inUse) = (make, inUse)
        self = Self()
        (self.make, self.inUse) = (make, inUse)
    }

    /// A surface neither shown nor in use, the one with the newest frame first, or a new one while fewer than
    /// `limit` exist. nil: none is free now, or none can be made (`failed`).
    private mutating func writable() -> Int? {
        let free = surfaces.indices.filter { $0 != front && !inUse(surfaces[$0]) }
        if let newest = free.max(by: { (holds[$0] ?? 0) < (holds[$1] ?? 0) }) {
            return newest
        }
        guard surfaces.count < Self.limit else { return nil }
        guard let surface = make(width, height) else {
            failed = true
            return nil
        }
        surfaces.append(surface)
        holds.append(nil)
        return surfaces.count - 1
    }

    /// What `frame` changed since the frame `held`: its own damage and that of every frame shown in between,
    /// when all of it is trusted. Anything else is the whole frame.
    private func region(for frame: HvfFramebufferFrame, holding held: UInt64?) -> HvfFramebufferDamage {
        let whole = HvfFramebufferDamage.whole(width: frame.width, height: frame.height)
        guard var region = frame.damage, let held else { return whole }
        var sequence = frame.sequence &- 2
        while sequence != held {
            guard let damage = damages[sequence] else { return whole }
            region = region.union(damage)
            sequence &-= 2
        }
        return region
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
