import Darwin
import Foundation

enum LibraryDeletionPathReference {
    static func mayReference(_ path: String, removedEntry: URL) -> Bool {
        let entry = removedEntry.resolvingSymlinksInPath().standardizedFileURL.path
        let url = URL(fileURLWithPath: path)
        // Keep both: deleting an in-entry symlink destroys its saved path even
        // when its target is external; an external alias can resolve inside.
        for candidate in [url.standardizedFileURL.path, url.resolvingSymlinksInPath().standardizedFileURL.path] {
            if candidate == entry || candidate.hasPrefix(entry + "/") { return true }
        }
        var selected = stat()
        guard stat(removedEntry.path, &selected) == 0 else { return true }
        // Case and Unicode aliases depend on the filesystem. Walking the saved
        // path's parents also catches an aliased in-entry link to external data.
        var ancestor = url.path
        while true {
            var current = stat()
            if stat(ancestor, &current) == 0 {
                if current.st_dev == selected.st_dev && current.st_ino == selected.st_ino { return true }
            } else if errno != ENOENT {
                return true // An unreadable or cyclic path cannot establish independence.
            }
            let parent = (ancestor as NSString).deletingLastPathComponent
            if parent == ancestor { return ancestor != "/" }
            ancestor = parent
        }
    }
}
