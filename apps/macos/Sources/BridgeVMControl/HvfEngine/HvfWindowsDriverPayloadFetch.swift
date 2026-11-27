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
        /// `.default` follows the Mac's proxy settings; tests swap in a stub protocol.
        var configuration = URLSessionConfiguration.default
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

    static let unreachableMessage = "Couldn’t reach the driver download. Check your internet connection and try again."
    static let incompleteMessage = "The driver download didn’t complete. Try again later."
    /// Offline, DNS, refused, timed out or a failed TLS handshake (often a proxy or captive portal).
    private static let unreachable: Set<URLError.Code> = [.notConnectedToInternet, .networkConnectionLost,
        .cannotFindHost, .cannotConnectToHost, .dnsLookupFailed, .timedOut, .dataNotAllowed,
        .internationalRoamingOff, .secureConnectionFailed]

    /// Blocking (download and unzip); call it off the main actor. Cancelling the calling task
    /// stops the download and throws `CancellationError`. `progress` gets the download percent.
    static func prepare(source: Source = Source(), root: URL = defaultRoot,
                        progress: @escaping (Int) -> Void = { _ in }) throws -> Prepared {
        let fm = FileManager.default
        let folder = root.appendingPathComponent(source.name, isDirectory: true)
        do { try fm.createDirectory(at: folder, withIntermediateDirectories: true) } catch {
            throw Failure(message: "Couldn’t create the driver folder at \(folder.path).")
        }
        // A quit during a download (or a curl run of an older build) left its partial file here.
        for name in (try? fm.contentsOfDirectory(atPath: folder.path)) ?? [] where name.hasPrefix(".download-") {
            try? fm.removeItem(at: folder.appendingPathComponent(name))
        }
        let archive = folder.appendingPathComponent("archive.zip")
        if !matchesPins(archive, source) { try download(source, to: archive, progress: progress) }

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

    /// fedorapeople.org answers browser-like agents with an HTML bot check, hence the `BridgeVM`
    /// agent; the pins refuse such a page or a truncated file before it replaces the cache.
    /// A data task, not a download task: a cancelled download task leaves its CFNetworkDownload
    /// file in the temporary folder. The bytes go to a hidden partial file here instead, which
    /// this function removes and the next fetch removes after a quit.
    private static func download(_ source: Source, to archive: URL, progress: @escaping (Int) -> Void) throws {
        let partial = archive.deletingLastPathComponent().appendingPathComponent(".download-\(UUID().uuidString).zip")
        defer { try? FileManager.default.removeItem(at: partial) }
        let unsaved = Failure(message: "Couldn’t save the downloaded drivers to \(archive.path).")
        guard let url = URL(string: source.url), FileManager.default.createFile(atPath: partial.path, contents: nil),
              let file = FileHandle(forWritingAtPath: partial.path) else { throw unsaved }
        var request = URLRequest(url: url, timeoutInterval: 60) // a stall, not the whole transfer
        request.setValue("BridgeVM", forHTTPHeaderField: "User-Agent")
        let transfer = Transfer(file: file, bytes: source.bytes, progress: progress)
        let session = URLSession(configuration: source.configuration, delegate: transfer, delegateQueue: nil)
        defer { session.finishTasksAndInvalidate() }
        let task = session.dataTask(with: request)
        task.resume()
        while transfer.done.wait(timeout: .now() + 0.2) == .timedOut {
            if Task.isCancelled { task.cancel() }
        }
        try? file.close()
        if let error = transfer.error {
            guard let code = (error as? URLError)?.code else { throw unsaved }
            if code == .cancelled { throw CancellationError() }
            throw Failure(message: unreachable.contains(code) ? unreachableMessage : incompleteMessage)
        }
        if let status = (task.response as? HTTPURLResponse)?.statusCode, !(200..<300).contains(status) {
            throw Failure(message: incompleteMessage)
        }
        guard matchesPins(partial, source) else {
            throw Failure(message: "The downloaded driver archive failed size or SHA-256 verification. Try downloading it again.")
        }
        guard rename(partial.path, archive.path) == 0 else { throw unsaved }
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

    /// Delegate callbacks arrive on the session's serial queue; `error` is read only after `done`.
    private final class Transfer: NSObject, URLSessionDataDelegate, @unchecked Sendable {
        let done = DispatchSemaphore(value: 0)
        let file: FileHandle
        let bytes: Int
        let progress: (Int) -> Void
        private var written = 0
        private var percent = -1
        private(set) var error: Error?

        init(file: FileHandle, bytes: Int, progress: @escaping (Int) -> Void) {
            (self.file, self.bytes, self.progress) = (file, bytes, progress)
        }

        func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
            do { try file.write(contentsOf: data) } catch {
                self.error = error // a full disk; the cancellation that follows must not hide it
                return dataTask.cancel()
            }
            written += data.count
            let now = min(100, written * 100 / max(bytes, 1))
            if now != percent { percent = now; progress(now) }
        }

        func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
            self.error = self.error ?? error
            done.signal()
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
