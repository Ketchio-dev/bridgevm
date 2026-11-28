import Darwin
import Foundation

/// A disk another app has open, as another VM app does while its VM runs: a copy taken then can hold half-written
/// data. Only this user's processes are listed (others need root), which is where VM apps run; watchers that
/// open a file only for change events (Finder, Spotlight) are left out.
enum HvfMediaSourceHolder {
    struct InUse: Error {}

    static func othersHaveOpen(_ path: String) -> Bool {
        var pids = [pid_t](repeating: 0, count: 2048)
        let bytes = proc_listpidspath(UInt32(PROC_ALL_PIDS), 0, path, UInt32(PROC_LISTPIDSPATH_EXCLUDE_EVTONLY),
                                      &pids, Int32(pids.count * MemoryLayout<pid_t>.stride))
        guard bytes > 0 else { return false }
        let own = getpid()
        return pids.prefix(Int(bytes) / MemoryLayout<pid_t>.stride).contains { $0 > 0 && $0 != own }
    }
}
