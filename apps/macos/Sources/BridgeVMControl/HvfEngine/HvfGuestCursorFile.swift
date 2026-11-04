#if canImport(AppKit)
import Darwin
import _Builtin_stdatomic

/// The guest's hardware cursor as the virtio-gpu device publishes it in `display.cursor`, next to
/// display.fb (`crates/bridgevm-hvf/src/virtio_gpu/cursor_file.rs`): magic, version 1, width, height,
/// stride and fourcc AR24 at 0..24, the seqlock at 24, the shape serial at 32, flags at 36 (bit 0 the
/// guest shows its sprite, bit 1 an image arrived in this boot), the sprite's top left in guest pixels at
/// 40/44 (the pointer is there plus the hot spot), the publish time at 48, the hot spot at 56/60 and the
/// BGRA image (straight alpha) from 64.
struct HvfGuestCursorState: Equatable {
    var serial: UInt32
    var visible: Bool
    var shapeThisBoot: Bool
    var x: Int32
    var y: Int32
    var hotX: Int
    var hotY: Int
    var width: Int
    var height: Int
}

enum HvfGuestCursorFile {
    static let name = "display.cursor"
    static let magic: UInt32 = 0x4256_4352
    static let fourcc: UInt32 = 0x3432_5241
    static let headerLength = 64
    static let maximumSide = 64
    static let flagVisible: UInt32 = 1
    static let flagShapeThisBoot: UInt32 = 2

    static func path(evidenceDir: String) -> String { evidenceDir + "/" + name }

    /// The header's state, or nil for a file this reader must not trust: never sized by its contents
    /// beyond 64x64 and the mapping's length.
    static func state(_ pointer: UnsafeRawPointer, length: Int) -> HvfGuestCursorState? {
        let word = { HvfFramebufferFrameReader.readUInt32(pointer, at: $0) }
        let width = Int(word(8)), height = Int(word(12)), stride = Int(word(16))
        let hotX = Int(word(56)), hotY = Int(word(60)), flags = word(36)
        guard length >= headerLength, word(0) == magic, word(4) == 1, word(20) == fourcc,
              (1...maximumSide).contains(width), (1...maximumSide).contains(height), stride == width * 4,
              length - headerLength >= height * stride, hotX < width, hotY < height else { return nil }
        return HvfGuestCursorState(serial: word(32), visible: flags & flagVisible != 0,
                                   shapeThisBoot: flags & flagShapeThisBoot != 0,
                                   x: Int32(bitPattern: word(40)), y: Int32(bitPattern: word(44)),
                                   hotX: hotX, hotY: hotY, width: width, height: height)
    }

    /// Whether the device took a cursor image in this boot: a hint read without the seqlock, for
    /// deciding whether the guest still needs its hardware cursor switched on.
    static func hasShapeThisBoot(path: String) -> Bool {
        let descriptor = HvfFramebufferFileIdentity.open(path)
        guard descriptor >= 0 else { return false }
        defer { Darwin.close(descriptor) }
        var header = [UInt8](repeating: 0, count: headerLength)
        guard Darwin.pread(descriptor, &header, headerLength, 0) == headerLength else { return false }
        return header.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress, let state = state(base, length: headerLength + 64 * 64 * 4)
            else { return false }
            return state.shapeThisBoot
        }
    }
}

/// Reads `display.cursor` under its seqlock, like the BVFB frame reader: a tick that finds the device
/// mid-write, or sees the sequence move during its copy, tries again at most `retries` times within
/// `budgetNanoseconds`. The image is copied only when the shape serial differs from the caller's.
struct HvfGuestCursorReader {
    enum Outcome: Equatable {
        /// A consistent state; `image` holds the BGRA pixels when the serial was not the caller's.
        case state(HvfGuestCursorState, sequence: UInt64, image: [UInt8]?)
        /// The state already taken.
        case unchanged
        /// The device held the state for the whole budget: keep what is shown.
        case busy
        /// Not a cursor file this reader trusts.
        case invalid
    }

    static let retries = 2
    static let budgetNanoseconds: UInt64 = 500_000

    var clock: () -> UInt64 = { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }
    var loadSequence: (UnsafeRawPointer) -> UInt64 = HvfFramebufferFrameReader.acquireSequence
    var pause: (_ writing: () -> Bool, _ expired: () -> Bool) -> Void = { writing, expired in
        while writing(), !expired() {}
    }

    func read(_ pointer: UnsafeRawPointer, length: Int, shown: UInt64, serial: UInt32?) -> Outcome {
        guard length >= HvfGuestCursorFile.headerLength,
              HvfFramebufferFrameReader.readUInt32(pointer, at: 0) == HvfGuestCursorFile.magic else {
            return .invalid
        }
        let deadline = clock() &+ Self.budgetNanoseconds
        var retried = 0
        while true {
            if let outcome = attempt(pointer, length: length, shown: shown, serial: serial) {
                return outcome
            }
            guard retried < Self.retries, clock() < deadline else { return .busy }
            retried += 1
            pause({ loadSequence(pointer) & 1 == 1 }, { clock() >= deadline })
        }
    }

    /// One seqlock read; nil when the device was writing or wrote during the copy.
    private func attempt(_ pointer: UnsafeRawPointer, length: Int, shown: UInt64,
                         serial: UInt32?) -> Outcome? {
        let sequence = loadSequence(pointer)
        guard sequence & 1 == 0 else { return nil }
        guard sequence != shown else { return .unchanged }
        let state = HvfGuestCursorFile.state(pointer, length: length)
        var image: [UInt8]?
        if let state, state.serial != serial {
            let count = state.height * state.width * 4
            image = [UInt8](unsafeUninitializedCapacity: count) { buffer, initialized in
                if let base = buffer.baseAddress {
                    Darwin.memcpy(base, pointer.advanced(by: HvfGuestCursorFile.headerLength), count)
                }
                initialized = count
            }
        }
        // No header or pixel read above may be satisfied after the sequence is read again.
        atomic_thread_fence(memory_order_acquire)
        guard loadSequence(pointer) == sequence else { return nil }
        guard let state else { return .invalid }
        return .state(state, sequence: sequence, image: image)
    }
}
#endif
