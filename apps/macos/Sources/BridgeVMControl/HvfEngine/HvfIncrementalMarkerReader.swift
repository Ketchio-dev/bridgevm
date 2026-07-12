import Foundation

final class HvfIncrementalMarkerReader {
    private struct FileGeneration: Equatable {
        let device: UInt64
        let inode: UInt64
    }

    private let marker: Data
    private let lock = NSLock()
    private var offset: UInt64 = 0
    private var carry = Data()
    private var found = false
    private var generation: FileGeneration?

    init(marker: String) {
        self.marker = Data(marker.utf8)
    }

    func reset() {
        lock.lock()
        defer { lock.unlock() }
        offset = 0
        carry.removeAll(keepingCapacity: true)
        found = false
        generation = nil
    }

    func containsMarker(in url: URL) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = (attributes[.size] as? NSNumber)?.uint64Value else { return false }
        let currentGeneration = FileGeneration(
            device: (attributes[.systemNumber] as? NSNumber)?.uint64Value ?? 0,
            inode: (attributes[.systemFileNumber] as? NSNumber)?.uint64Value ?? 0
        )
        if let generation, generation != currentGeneration {
            offset = 0
            carry.removeAll(keepingCapacity: true)
            found = false
        }
        generation = currentGeneration
        if found { return true }
        if size < offset {
            offset = 0
            carry.removeAll(keepingCapacity: true)
        }
        guard size > offset, let handle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: offset)
            let data = try handle.readToEnd() ?? Data()
            offset += UInt64(data.count)
            carry.append(data)
        } catch {
            return false
        }
        if carry.range(of: marker) != nil {
            found = true
            carry.removeAll(keepingCapacity: false)
            return true
        }
        let retainedCount = min(carry.count, max(0, marker.count - 1))
        carry = Data(carry.suffix(retainedCount))
        return false
    }
}
