import Foundation

enum VMResourceLimits {
    static let minimumMemoryMiB = 1_024
    static let maximumMemoryMiB = 32 * 1_024
    static let minimumCPU = 1
    /// At least 10, and on a larger Mac as many as Create and the CLI offer (all cores but one),
    /// so a VM made there can still be changed in Settings.
    static let maximumCPU = max(10, ProcessInfo.processInfo.activeProcessorCount - 1)

    static func contains(memoryMiB: Int, cpu: Int) -> Bool {
        (minimumMemoryMiB...maximumMemoryMiB).contains(memoryMiB)
            && (minimumCPU...maximumCPU).contains(cpu)
    }
}

// MARK: - Configuration
/// One controllable VM. Persisted per-VM as vm.json inside the library so the
/// app manages MANY VMs. `id`/`bootMode` are optional for backward-compat with
/// the legacy single ~/.bridgevm-control/config.json.
struct VMConfig: Codable, Identifiable, Equatable, Sendable {
    var id: String?                  // stable slug; falls back to slug(name)
    var name: String
    var displayName: String
    var backendKind: String          // see BackendKind
    var bootMode: String?            // "direct-kernel" (default) | "iso-efi"
    var bundlePath: String
    var runnerPath: String
    var launchSpecPath: String
    var handoffPath: String
    var sshKeyPath: String
    var sshUser: String
    var leasesPath: String
    var guestName: String
    var displayWidth: Int
    var displayHeight: Int
    var startWithBridgeVM: Bool? = nil
    var installPending: Bool? = nil  // true for a freshly-created ISO VM not yet installed
    var isoPath: String? = nil       // Windows install ISO
    var diskPath: String? = nil      // qcow2/raw install target
    var memMiB: Int? = nil
    var cpuCount: Int? = nil
    var networkEnabled: Bool? = nil  // nil == default on; false disables the guest NIC (HVF only)
    var experimental3DAllowed: Bool? = nil
    var virtioDisplayDriverInstalled: Bool? = nil  // nil == false; true: the Microsoft-signed virtio-win display driver is installed or staged in this guest, so a 3D-off launch attaches the 2D virtio-gpu (HVF only)
    var hardwareCursor: Bool? = nil  // nil == true: with that driver, Windows' pointer shows as the Mac cursor (the app sets viogpudo's HWCursor=1); false: the app writes HWCursor=0 instead (HVF only)
    var clipboardSyncEnabled: Bool? = nil  // nil == default on; false keeps two-way clipboard sync off (HVF only)
    var durableDiskWritesEnabled: Bool? = nil  // nil == off: guest flushes are ordered, durable within ~1 s; true: every flush waits for F_FULLFSYNC (HVF only)
    var retinaResolution: Bool? = nil  // nil == never set: plain window-size requests; true: on a HiDPI screen the 2D virtio display asks for the window's backing pixels at the screen's scale, up to a size the display driver takes; false == off again. Once set, every request names the Windows scale (100 % unless drawing at Retina) (HVF only)
    var microphoneEnabled: Bool? = nil  // nil == off: the guest microphone records silence; true: the Mac's default input while Windows records, with macOS's permission (HVF only)
    var printToMac: Bool? = nil  // nil == on (never set), as Parallels shares the Mac's printers: Windows gets the printer "Mac (BridgeVM)", whose jobs open in the Mac's print panel; true: on; false: off, the app removes that printer (HVF only)
    var openGuestLinksOnMac: Bool? = nil  // nil == off (never set); true: Windows' http/https links open in the Mac's default browser (the app registers a per-user BridgeVM browser in Windows); false == off again, the app removes that registration (HVF only)
    var guestResolution: String? = nil  // Windows display "WxH"; nil == automatic: the VMM's 1280x800 unless the VM's firmware already has one (HVF only)
    var commandKeyMode: String? = nil  // nil == Mac-like: Command chords are Ctrl chords, a bare Command tap is the Windows key; "windows": Command is the Windows key (HVF only)
    var sharedFolderHostPath: String? = nil, sharedFolderGuestPath: String? = nil  // nil == no sharing (HVF only)
    var createSharedFolderOnStart: Bool? = nil  // only new Windows VMs; never inferred for existing registrations
    var portForwards: String? = nil  // "8080:80, 3390:3389": the Mac's 127.0.0.1:8080 reaches the guest's TCP port 80; nil == none (HVF only)
    /// A config can be imported or hand-edited, so never trust its persisted ID
    /// as a filesystem component. Normalizing here keeps every caller inside the
    /// VM library even when an ID contains separators or traversal components.
    var slug: String { VMConfig.slugify(id ?? name) }
    var engineKind: BackendKind { BackendKind(savedKind: backendKind) }
    var engineShortLabel: String { engineKind.shortLabel }
    var engineDetailLabel: String { engineKind.detailLabel }

    static func slugify(_ s: String) -> String {
        var out = ""
        for ch in s.lowercased() {
            if ch.isLetter || ch.isNumber { out.append(ch) }
            else { out.append("-") }
        }
        while out.contains("--") { out = out.replacingOccurrences(of: "--", with: "-") }
        let trimmed = out.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return trimmed.isEmpty ? "vm" : trimmed   // never an empty slug (would break the library path)
    }

    /// Legacy single-VM config (used only for one-time migration into the library).
    static func loadLegacy() -> VMConfig? {
        let path = ("~/.bridgevm-control/config.json" as NSString).expandingTildeInPath
        guard let data = FileManager.default.contents(atPath: path) else { return nil }
        return try? JSONDecoder().decode(VMConfig.self, from: data)
    }
}

extension VMConfig {
    /// Resolve the concrete backend for this VM (the engine seam).
    func makeBackend(libraryRoot: URL = VMLibrary.root) -> VMBackend {
        switch engineKind {
        case .hvfEngine: return HvfWindowsBackend(self, libraryRoot: libraryRoot)
        case .unsupported: return UnsupportedEngineBackend(self)
        }
    }
}

// MARK: - Shell helper

enum Shell {
    private final class BoundedOutputCollector {
        private let lock = NSLock()
        private let limit: Int
        private var chunks: [Data] = []
        private var byteCount = 0
        private var truncated = false

        init(limit: Int) { self.limit = max(1, limit) }

        func append(_ data: Data) {
            guard !data.isEmpty else { return }
            lock.lock()
            defer { lock.unlock() }
            chunks.append(data)
            byteCount += data.count
            while byteCount > limit, !chunks.isEmpty {
                truncated = true
                let excess = byteCount - limit
                if chunks[0].count <= excess {
                    byteCount -= chunks.removeFirst().count
                } else {
                    chunks[0].removeFirst(excess)
                    byteCount -= excess
                }
            }
        }

        func snapshot() -> (data: Data, truncated: Bool) {
            lock.lock()
            defer { lock.unlock() }
            var data = Data(capacity: byteCount)
            for chunk in chunks { data.append(chunk) }
            return (data, truncated)
        }
    }

    /// Run a command to completion (bounded by `timeout`) and return its combined
    /// stdout+stderr and exit code. Output is drained on a background queue so a
    /// child that writes more than the pipe buffer can never block/deadlock.
    @discardableResult
    static func run(
        _ launchPath: String,
        _ args: [String],
        timeout: Double = 30,
        outputLimitBytes: Int = 4 * 1024 * 1024
    ) -> (output: String, code: Int32) {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: launchPath)
        proc.arguments = args
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = pipe

        let collector = BoundedOutputCollector(limit: outputLimitBytes)
        let readHandle = pipe.fileHandleForReading
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                while let chunk = try readHandle.read(upToCount: 64 * 1024), !chunk.isEmpty {
                    collector.append(chunk)
                }
            } catch {
                // Closing the handle is the intentional escape hatch when a
                // grandchild keeps the inherited write descriptor alive.
            }
            group.leave()
        }
        // Wait on the child's own termination instead of sampling isRunning in
        // a 50 ms sleep loop, which added half that period to every command on
        // average and dominated short ones: pgrep measured 127 ms through here
        // against 23 ms waiting on this semaphore. Installed before run() so
        // the callback cannot be attached after the child has already exited.
        let exited = DispatchSemaphore(value: 0)
        proc.terminationHandler = { _ in exited.signal() }
        do {
            try proc.run()
        } catch {
            try? pipe.fileHandleForWriting.close()
            try? readHandle.close()
            _ = group.wait(timeout: .now() + 1)
            return ("Couldn’t run \(launchPath): \(error.localizedDescription)", -1)
        }
        // Process has inherited/duplicated the write descriptor; the parent copy
        // must close or EOF can never arrive after the child exits.
        try? pipe.fileHandleForWriting.close()
        _ = exited.wait(timeout: .now() + timeout)
        if proc.isRunning {
            let descendants = descendantProcessIDs(of: proc.processIdentifier)
            for pid in descendants.reversed() { kill(pid, SIGTERM) }
            proc.terminate()
            usleep(300_000)
            for pid in descendants.reversed() where kill(pid, 0) == 0 { kill(pid, SIGKILL) }
            if proc.isRunning { kill(proc.processIdentifier, SIGKILL) }
        }
        proc.waitUntilExit()
        if group.wait(timeout: .now() + 0.25) == .timedOut {
            // A grandchild may have inherited stdout. Do not leave a reader
            // mutating shared output after this function returns.
            try? readHandle.close()
            _ = group.wait(timeout: .now() + 1)
        }
        let snapshot = collector.snapshot()
        let body = String(decoding: snapshot.data, as: UTF8.self)
        let output = snapshot.truncated
            ? "[Output truncated; last \(snapshot.data.count) bytes]\n" + body
            : body
        return (output, proc.terminationStatus)
    }

    private static func descendantProcessIDs(of root: pid_t) -> [pid_t] {
        var ordered: [pid_t] = []
        var visited = Set<pid_t>([root])
        var queue = [root]
        while !queue.isEmpty {
            let parent = queue.removeFirst()
            for child in directChildProcessIDs(of: parent) where visited.insert(child).inserted {
                ordered.append(child)
                queue.append(child)
            }
        }
        return ordered
    }

    private static func directChildProcessIDs(of parent: pid_t) -> [pid_t] {
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
        task.arguments = ["-P", String(parent)]
        let pipe = Pipe()
        task.standardOutput = pipe
        task.standardError = FileHandle.nullDevice
        do {
            try task.run()
            try? pipe.fileHandleForWriting.close()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            task.waitUntilExit()
            guard task.terminationStatus == 0 else { return [] }
            return String(decoding: data, as: UTF8.self)
                .split(whereSeparator: \.isWhitespace)
                .compactMap { pid_t($0) }
        } catch {
            return []
        }
    }

    /// Escape ERE metacharacters so a literal string (e.g. a filesystem path) can
    /// be matched exactly by `pgrep -f` / `pkill -f` (which treat it as a regex).
    static func eregEscape(_ s: String) -> String {
        let meta = Set("\\^$.[]|()*+?{}")
        var out = ""
        for ch in s { if meta.contains(ch) { out.append("\\") }; out.append(ch) }
        return out
    }

    private static func processIDs(matching pattern: String) -> [pid_t] {
        guard !pattern.isEmpty else { return [] }
        let result = run("/usr/bin/pgrep", ["-f", eregEscape(pattern)])
        guard result.code == 0 else { return [] }
        return result.output.split(whereSeparator: \.isWhitespace).compactMap { pid_t($0) }
    }

    /// BridgeVM's disk helper (a snapshot, export, disk grow or media lease, also from the CLI)
    /// names the disk in its arguments too, but it is not the VM: the media lock keeps them apart.
    private static func isDiskHelper(_ pid: pid_t) -> Bool {
        var path = [CChar](repeating: 0, count: 4 * Int(MAXPATHLEN))
        guard proc_pidpath(pid, &path, UInt32(path.count)) > 0 else { return false }
        return URL(fileURLWithPath: String(cString: path)).lastPathComponent == "snapshot_pair_cli"
    }

    /// Whether a process other than the disk helper has `pattern` (a VM's disk) on its command line.
    static func isProcessRunning(matching pattern: String) -> Bool {
        processIDs(matching: pattern).contains { !isDiskHelper($0) }
    }

    static func isDiskHelperRunning(matching pattern: String) -> Bool {
        processIDs(matching: pattern).contains(where: isDiskHelper)
    }

    /// Ends the VM process using the disk, never a helper in the middle of a copy.
    static func killProcesses(matching pattern: String) {
        for pid in processIDs(matching: pattern) where !isDiskHelper(pid) { kill(pid, SIGTERM) }
    }
}

enum VMDiskHelper {
    static let busy = "Another BridgeVM task, such as a snapshot or export, is using this VM’s disk. Try again when it finishes."
}

// MARK: - Backend abstraction

/// What the library polls for each VM. HvfWindowsBackend is the only engine;
/// UnsupportedEngineBackend stands in for a VM saved with a removed one. Starting,
/// stopping and guest commands belong to HvfEngineSession.
protocol VMBackend: AnyObject {
    /// Whether the VM is running: a process other than BridgeVM's disk helper uses its disk,
    /// wherever it was started.
    func isRunning() -> Bool
    /// Whether the disk helper (a snapshot or export, maybe from the CLI) is using the disk.
    func diskHelperIsRunning() -> Bool
    func resources() -> (memMiB: Int, cpu: Int)
}

extension VMBackend {
    func diskHelperIsRunning() -> Bool { false }
}
