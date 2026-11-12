import Foundation

/// A 1 MiB QCOW2 v3 image (64 KiB clusters) with one data cluster at virtual cluster 3, and the
/// RAW disk it describes, for driving the real helper's `convert-to-raw` through the import paths.
enum HvfQCOW2TestImage {
    static let cluster = 65_536
    static let virtualBytes = 1 << 20
    static let data = Data((0..<cluster).map { UInt8(truncatingIfNeeded: $0 &* 7 &+ 1) })

    static func make(backingFile: Bool = false) -> (image: Data, raw: Data) {
        var image = Data(count: 4 * cluster)
        func put(_ value: UInt64, at offset: Int, width: Int) {
            for i in 0..<width { image[offset + i] = UInt8(truncatingIfNeeded: value >> (8 * UInt64(width - 1 - i))) }
        }
        put(0x5146_49fb, at: 0, width: 4)
        put(3, at: 4, width: 4)
        if backingFile { put(UInt64(3 * cluster), at: 8, width: 8) }
        put(16, at: 20, width: 4)
        put(UInt64(virtualBytes), at: 24, width: 8)
        put(1, at: 36, width: 4)  // L1 entries
        put(UInt64(cluster), at: 40, width: 8)  // L1 at cluster 1
        put(4, at: 96, width: 4)  // refcount order
        put(104, at: 100, width: 4)  // header length
        put(UInt64(2 * cluster) | 1 << 63, at: cluster, width: 8)  // L2 at cluster 2
        put(UInt64(3 * cluster) | 1 << 63, at: 2 * cluster + 3 * 8, width: 8)  // data at cluster 3
        image.replaceSubrange(3 * cluster..<4 * cluster, with: data)
        var raw = Data(count: virtualBytes)
        raw.replaceSubrange(3 * cluster..<4 * cluster, with: data)
        return (image, raw)
    }
}
