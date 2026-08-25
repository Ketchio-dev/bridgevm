import Foundation

enum HvfWindowsMediaPublicationTransaction {
    typealias Item = (destination: String, source: String)
    typealias Staged = HvfWindowsAtomicMediaPublisher.Staged

    static func publish(disk: Item, vars: Item) throws {
        try publish([disk, vars], swapping: HvfWindowsAtomicMediaPublisher.swap)
    }

    static func publish(
        _ items: [Item], swapping: (Staged) throws -> Void
    ) throws {
        let fm = FileManager.default
        var staged: [Staged] = []
        var swapped = 0
        do {
            for item in items {
                staged.append(try HvfWindowsAtomicMediaPublisher.stage(
                    destination: item.destination, source: item.source))
            }
            for item in staged {
                try swapping(item)
                swapped += 1
            }
        } catch {
            do {
                for item in staged.prefix(swapped).reversed() {
                    try HvfWindowsAtomicMediaPublisher.rollback(item)
                }
            } catch {
                throw HvfWindowsAtomicMediaPublisher.PublicationError.rollbackFailed
            }
            for item in staged { try? fm.removeItem(atPath: item.temporary) }
            throw error
        }
        for item in staged {
            try? fm.removeItem(atPath: item.temporary)
            try? fm.removeItem(atPath: item.source)
        }
    }
}
