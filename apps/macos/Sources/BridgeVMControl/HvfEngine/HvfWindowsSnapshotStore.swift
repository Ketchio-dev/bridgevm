import Foundation

/// Names are display text in a sibling file, never part of a snapshot path or its helper format.
enum HvfWindowsSnapshotStore {
    struct Entry: Identifiable, Equatable {
        let url: URL
        let date: Date
        let name: String?
        var id: String { url.lastPathComponent }
        var label: String {
            let dateText = HvfWindowsSnapshotModel.dateText(date)
            return name.map { "\($0) · \(dateText)" } ?? dateText
        }
    }

    static func directory(bundle: URL) -> URL {
        bundle.appendingPathComponent("metadata/snapshots", isDirectory: true)
    }

    static func nameFile(_ snapshot: URL) -> URL { snapshot.appendingPathExtension("name") }

    static func list(bundle: URL, fileManager: FileManager = .default) -> [Entry] {
        let directory = directory(bundle: bundle)
        guard HvfWindowsSnapshotCommand.canonical(directory),
              let children = try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) else { return [] }
        return children.compactMap { child -> Entry? in
            let url = directory.appendingPathComponent(child.lastPathComponent).standardizedFileURL
            guard url.pathExtension == "snapshot", (try? validatedTarget(url, bundle: bundle)) != nil,
                  HvfWindowsSnapshotCommand.regularFile(url.appendingPathComponent("manifest.json")),
                  let manifestDate = try? url.appendingPathComponent("manifest.json")
                    .resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate else { return nil }
            let date = timestamp(url.lastPathComponent) ?? manifestDate
            let sidecar = nameFile(url)
            let name = HvfWindowsSnapshotCommand.regularFile(sidecar)
                ? try? String(contentsOf: sidecar, encoding: .utf8) : nil
            return Entry(url: url, date: date, name: name?.isEmpty == false ? name : nil)
        }.sorted { $0.date == $1.date ? $0.id > $1.id : $0.date > $1.date }
    }

    /// Timestamped snapshots keep their creation order when files are copied or touched.
    static func timestamp(_ filename: String) -> Date? {
        guard filename.range(of: #"^[0-9]{8}-[0-9]{6}\.snapshot$"#, options: .regularExpression) != nil else { return nil }
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        formatter.isLenient = false
        return formatter.date(from: String(filename.prefix(15)))
    }

    /// Move forward one second on a collision, keeping the UTC filename contract even for rapid clicks.
    static func nextURL(bundle: URL, now: Date = Date(), occupied: Set<String>) -> URL {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        var date = now
        while true {
            let name = formatter.string(from: date) + ".snapshot"
            if !occupied.contains(name), !occupied.contains(name + ".name") {
                return directory(bundle: bundle).appendingPathComponent(name)
            }
            date = date.addingTimeInterval(1)
        }
    }

    /// Only an existing, non-symlink direct child of this VM's canonical snapshot directory is mutable.
    static func validatedTarget(_ target: URL, bundle: URL) throws -> URL {
        let directory = directory(bundle: bundle).standardizedFileURL
        let target = target.standardizedFileURL
        guard target.deletingLastPathComponent() == directory, target.pathExtension == "snapshot",
              HvfWindowsSnapshotCommand.canonical(directory), HvfWindowsSnapshotCommand.canonical(target),
              HvfWindowsSnapshotCommand.regularDirectory(target) else {
            throw HvfWindowsSnapshotCommand.failure("snapshot target is outside its directory or unsafe", .unsafeFolder)
        }
        return target
    }

    static func delete(_ target: URL, bundle: URL) throws {
        let target = try validatedTarget(target, bundle: bundle)
        try FileManager.default.removeItem(at: target)
        let sidecar = nameFile(target)
        if HvfWindowsSnapshotCommand.regularFile(sidecar) { try? FileManager.default.removeItem(at: sidecar) }
    }
}
