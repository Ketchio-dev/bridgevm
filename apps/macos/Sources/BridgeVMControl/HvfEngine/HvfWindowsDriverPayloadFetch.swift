import Darwin
import Foundation

/// Builds the storage, serial and network guest payload for an ISO install from the pinned,
/// Microsoft-signed Red Hat virtio-win archive the display driver install already uses, so an
/// ISO alone is enough. The archive is downloaded once, refused unless its length and SHA-256
/// match the pins, and kept; every call rebuilds the payload folder and its manifest from it, and
/// the result feeds the normal payload path, which copies and seals both into the VM bundle.
enum HvfWindowsDriverPayloadFetch {
    struct Source {
        var url = HvfDisplayDriverInstall.archiveURL
        var bytes = HvfDisplayDriverInstall.archiveBytes
        var sha256 = HvfDisplayDriverInstall.archiveSHA256
        var folder = HvfDisplayDriverInstall.archiveFolder
        /// The archive's file name without `.zip`, so a new pinned archive gets its own folder.
        var name: String { ((URL(string: url)?.lastPathComponent ?? "virtio-win") as NSString).deletingPathExtension }
    }

    struct Prepared: Equatable {
        let payloadDirectory: String
        let manifestPath: String
    }

    struct Failure: Error, Equatable {
        let message: String
    }

    /// Each role's folder with its INF, catalog and the binaries the INF copies (netkvm.inf
    /// copies netkvmp.exe, not netkvmco).
    static let drivers: [(role: String, files: [String])] = [
        ("storage", ["viostor.inf", "viostor.cat", "viostor.sys"]),
        ("serial", ["vioser.inf", "vioser.cat", "vioser.sys"]),
        ("network", ["netkvm.inf", "netkvm.cat", "netkvm.sys", "netkvmp.exe"]),
    ]

    /// `~/Library/Application Support/BridgeVM/driver-payloads`.
    static var defaultRoot: URL {
        VMLibrary.root.deletingLastPathComponent().appendingPathComponent("driver-payloads", isDirectory: true)
    }

    /// Blocking (download and unzip); call it off the main actor.
    static func prepare(source: Source = Source(), root: URL = defaultRoot) throws -> Prepared {
        let fm = FileManager.default
        let folder = root.appendingPathComponent(source.name, isDirectory: true)
        do { try fm.createDirectory(at: folder, withIntermediateDirectories: true) } catch {
            throw Failure(message: "Couldn’t create the driver folder at \(folder.path).")
        }
        let archive = folder.appendingPathComponent("archive.zip")
        if !matchesPins(archive, source) { try download(source, to: archive) }

        let staging = folder.appendingPathComponent(".staging-\(UUID().uuidString)", isDirectory: true)
        defer { try? fm.removeItem(at: staging) }
        let stagedPayload = staging.appendingPathComponent("payload", isDirectory: true)
        let stagedManifest = staging.appendingPathComponent("manifest.tsv")
        var lines = ["schema\tbridgevm-windows-guest-payload-v1", "architecture\tarm64"]
        var paths: [String] = []
        for driver in drivers {
            let directory = stagedPayload.appendingPathComponent(driver.role, isDirectory: true)
            do { try fm.createDirectory(at: directory, withIntermediateDirectories: true) } catch {
                throw Failure(message: "Couldn’t create the driver folder at \(directory.path).")
            }
            for file in driver.files {
                try extract(source.folder + file, from: archive, to: directory.appendingPathComponent(file))
            }
            let relative = driver.files.map { "\(driver.role)/\($0)" }
            paths += relative
            lines.append((["driver", driver.role, relative[0], relative[1]]
                + [relative[2...].joined(separator: ",")]).joined(separator: "\t"))
        }
        for path in paths.sorted() {
            guard let digest = HvfWindowsInstallCacheIdentity.sha256File(stagedPayload.appendingPathComponent(path).path) else {
                throw Failure(message: "Couldn’t read the extracted driver file at \(path).")
            }
            lines.append("file\t\(path)\t\(digest)")
        }
        let payload = folder.appendingPathComponent("payload", isDirectory: true)
        let manifest = folder.appendingPathComponent("manifest.tsv")
        do {
            try Data((lines.joined(separator: "\n") + "\n").utf8).write(to: stagedManifest)
            try place(stagedPayload, at: payload)
            try place(stagedManifest, at: manifest)
        } catch {
            throw Failure(message: "Couldn’t save the drivers to \(folder.path).")
        }
        return Prepared(payloadDirectory: payload.path, manifestPath: manifest.path)
    }

    private static func matchesPins(_ archive: URL, _ source: Source) -> Bool {
        let size = (try? archive.resourceValues(forKeys: [.fileSizeKey]))?.fileSize
        return size == source.bytes && HvfWindowsInstallCacheIdentity.sha256File(archive.path) == source.sha256
    }

    /// fedorapeople.org answers browser-like agents with an HTML bot check, hence `-A BridgeVM`;
    /// the pins refuse such a page or a truncated file before it replaces the cache.
    private static func download(_ source: Source, to archive: URL) throws {
        let partial = archive.deletingLastPathComponent().appendingPathComponent(".download-\(UUID().uuidString).zip")
        defer { try? FileManager.default.removeItem(at: partial) }
        let status = run("/usr/bin/curl", ["-sS", "-A", "BridgeVM", "--fail", "--location",
            "--connect-timeout", "30", "--speed-limit", "1024", "--speed-time", "60", "--max-time", "1800",
            "-o", partial.path, source.url])
        guard status == 0 else {
            throw Failure(message: "Driver download failed (curl \(status)). Check your internet connection and try again.")
        }
        guard matchesPins(partial, source) else {
            throw Failure(message: "The downloaded driver archive failed size or SHA-256 verification. Try downloading it again.")
        }
        guard rename(partial.path, archive.path) == 0 else {
            throw Failure(message: "Couldn’t save the downloaded drivers to \(archive.path).")
        }
    }

    private static func extract(_ member: String, from archive: URL, to file: URL) throws {
        guard FileManager.default.createFile(atPath: file.path, contents: nil),
              let handle = FileHandle(forWritingAtPath: file.path) else {
            throw Failure(message: "Couldn’t create the driver file at \(file.path).")
        }
        let status = run("/usr/bin/unzip", ["-p", archive.path, member], output: handle)
        try? handle.close()
        let size = (try? file.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
        guard status == 0, size > 0 else {
            throw Failure(message: "Couldn’t extract \(member) from the driver archive.")
        }
    }

    private static func place(_ item: URL, at destination: URL) throws {
        if FileManager.default.fileExists(atPath: destination.path) {
            _ = try FileManager.default.replaceItemAt(destination, withItemAt: item)
        } else {
            try FileManager.default.moveItem(at: item, to: destination)
        }
    }

    private static func run(_ tool: String, _ arguments: [String], output: FileHandle? = nil) -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: tool)
        process.arguments = arguments
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = output ?? FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return -1 }
        process.waitUntilExit()
        return process.terminationStatus
    }
}
