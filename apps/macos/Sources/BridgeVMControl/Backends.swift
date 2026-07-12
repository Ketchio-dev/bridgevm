import Foundation

enum VMResourceLimits {
    static let minimumMemoryMiB = 1_024
    static let maximumMemoryMiB = 32 * 1_024
    static let minimumCPU = 1
    static let maximumCPU = 10

    static func contains(memoryMiB: Int, cpu: Int) -> Bool {
        (minimumMemoryMiB...maximumMemoryMiB).contains(memoryMiB)
            && (minimumCPU...maximumCPU).contains(cpu)
    }
}

// MARK: - Configuration

/// One controllable VM. Persisted per-VM as vm.json inside the library so the
/// app manages MANY VMs. `id`/`bootMode` are optional for backward-compat with
/// the legacy single ~/.bridgevm-control/config.json.
struct VMConfig: Codable, Identifiable, Equatable {
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
    var installPending: Bool? = nil  // true for a freshly-created ISO VM not yet installed
    // QEMU (qemu-compat) backend fields:
    var isoPath: String? = nil       // install ISO (Windows / Linux)
    var diskPath: String? = nil      // qcow2/raw install target
    var memMiB: Int? = nil
    var cpuCount: Int? = nil

    /// A config can be imported or hand-edited, so never trust its persisted ID
    /// as a filesystem component. Normalizing here keeps every caller inside the
    /// VM library even when an ID contains separators or traversal components.
    var slug: String { VMConfig.slugify(id ?? name) }
    var effectiveBootMode: String { bootMode ?? "direct-kernel" }
    var engineKind: BackendKind { BackendKind(rawValue: backendKind) ?? .fastVZ }
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

/// The engine a VM runs on — the single source of truth for the kind strings and
/// their display labels (previously duplicated across views/models).
enum BackendKind: String {
    case fastVZ = "fast-vz"
    case qemuCompat = "qemu-compat"
    case hvfEngine = "hvf-engine"

    var shortLabel: String {
        switch self { case .fastVZ: return "Fast VZ"; case .qemuCompat: return "QEMU"; case .hvfEngine: return "HVF" }
    }
    var detailLabel: String {
        switch self {
        case .fastVZ: return "Fast (Apple VZ)"
        case .qemuCompat: return "Compatibility (QEMU)"
        case .hvfEngine: return "Native (HVF · Preview)"
        }
    }
    /// Whether this engine is actually selectable/usable today.
    var available: Bool { self == .fastVZ || self == .qemuCompat || self == .hvfEngine }
}

extension VMConfig {
    /// Resolve the concrete backend for this VM (the engine seam).
    func makeBackend() -> VMBackend {
        switch engineKind {
        case .fastVZ: return FastVZBackend(self)
        case .qemuCompat: return QemuCompatBackend(self)
        case .hvfEngine: return HvfWindowsBackend(self)
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
        do {
            try proc.run()
        } catch {
            try? pipe.fileHandleForWriting.close()
            try? readHandle.close()
            _ = group.wait(timeout: .now() + 1)
            return ("실행 실패: \(error.localizedDescription)", -1)
        }
        // Process has inherited/duplicated the write descriptor; the parent copy
        // must close or EOF can never arrive after the child exits.
        try? pipe.fileHandleForWriting.close()
        let deadline = Date().addingTimeInterval(timeout)
        while proc.isRunning && Date() < deadline { usleep(50_000) }
        if proc.isRunning {
            proc.terminate()
            usleep(300_000)
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
            ? "[출력 일부 생략 — 마지막 \(snapshot.data.count)바이트]\n" + body
            : body
        return (output, proc.terminationStatus)
    }

    /// Launch a long-running process fully detached from this app (its own window).
    @discardableResult
    static func launchDetached(_ command: String) -> Bool {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/sh")
        proc.arguments = ["-c", command]
        do {
            try proc.run()
            return true
        } catch {
            return false
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

    static func isProcessRunning(matching pattern: String) -> Bool {
        guard !pattern.isEmpty else { return false }
        return run("/usr/bin/pgrep", ["-f", eregEscape(pattern)]).code == 0
    }

    static func killProcesses(matching pattern: String) {
        guard !pattern.isEmpty else { return }
        run("/usr/bin/pkill", ["-f", eregEscape(pattern)])
    }

    static func shQuote(_ s: String) -> String {
        "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// Render an executable and argv for `/bin/sh` without allowing any argument
    /// content to become syntax. Keep shell use at the orchestration boundary;
    /// all user/config-derived values must enter through this helper.
    static func shellCommand(_ executable: String, _ args: [String]) -> String {
        ([executable] + args).map(shQuote).joined(separator: " ")
    }
}

// MARK: - Backend abstraction

/// The single control surface every engine plugs into. FastVZBackend (Apple VZ
/// Linux) and QemuCompatBackend (QEMU Windows) are wired; future engines conform
/// to the same protocol so the app stays one unified front-end.
protocol VMBackend: AnyObject {
    var displayName: String { get }
    var kind: String { get }
    func isRunning() -> Bool
    func currentIP() -> String?
    /// Returns true when the host-side launch request was accepted. This does not
    /// imply that the guest has finished booting, only that startup was dispatched.
    @discardableResult func start() -> Bool
    func stop()
    func resources() -> (memMiB: Int, cpu: Int)
    func setResources(memMiB: Int, cpu: Int) -> Bool
    /// Run a shell command inside the guest, return its combined output.
    func runInGuest(_ command: String) -> (output: String, code: Int32)
    /// True when a control channel into the guest (e.g. SSH) is available — gates
    /// the in-app terminal and software-install UI.
    var supportsGuestCommands: Bool { get }
    var supportsPackageInstall: Bool { get }
    var supportsClipboard: Bool { get }
    var supportsSSH: Bool { get }
    var supportsResourceChanges: Bool { get }
}

// MARK: - Fast Mode (Apple Virtualization.framework) backend

final class FastVZBackend: VMBackend {
    let config: VMConfig
    init(_ config: VMConfig) { self.config = config }

    var displayName: String { config.displayName }
    let kind = "fast-vz"
    let supportsGuestCommands = true
    let supportsPackageInstall = true
    let supportsClipboard = true
    let supportsSSH = true
    let supportsResourceChanges = true

    // Per-VM identity: match THIS VM's UNIQUE handoff path on the runner's command
    // line, so start/stop/status never touch another VM's runner.
    private var runnerPattern: String { config.handoffPath }

    func isRunning() -> Bool { Shell.isProcessRunning(matching: runnerPattern) }
    func stop() { Shell.killProcesses(matching: runnerPattern) }

    func currentIP() -> String? {
        guard let text = try? String(contentsOfFile: config.leasesPath, encoding: .utf8) else { return nil }
        // Find the lease block whose name= matches guestName, then its ip_address=.
        var matched = false
        for raw in text.split(separator: "\n") {
            let line = raw.trimmingCharacters(in: .whitespaces)
            if line.hasPrefix("name=") { matched = (line == "name=\(config.guestName)") }
            if matched, line.hasPrefix("ip_address=") {
                let candidate = String(line.dropFirst("ip_address=".count))
                return Self.isValidIPv4(candidate) ? candidate : nil
            }
        }
        return nil
    }

    private static func isValidIPv4(_ value: String) -> Bool {
        let parts = value.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return false }
        return parts.allSatisfy { part in
            guard !part.isEmpty, part.allSatisfy(\.isNumber), let number = Int(part) else { return false }
            return number >= 0 && number <= 255
        }
    }

    private var hasValidSSHUser: Bool {
        guard let first = config.sshUser.first, first.isLetter || first.isNumber || first == "_" else { return false }
        return config.sshUser.allSatisfy { $0.isLetter || $0.isNumber || $0 == "_" || $0 == "-" || $0 == "." }
    }

    @discardableResult func start() -> Bool {
        guard FileManager.default.isExecutableFile(atPath: config.runnerPath),
              FileManager.default.isReadableFile(atPath: config.handoffPath),
              config.displayWidth > 0, config.displayHeight > 0 else { return false }
        return Shell.launchDetached(launchCommand())
    }

    func launchCommand() -> String {
        // Launch the signed AppleVzRunner with its own display window, detached.
        let args = ["--display", "--display-width", String(config.displayWidth),
                    "--display-height", String(config.displayHeight), "--allow-real-vz-start",
                    "--handoff-json", config.handoffPath]
        return "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 nohup \(Shell.shellCommand(config.runnerPath, args)) >/dev/null 2>&1 &"
    }

    func resources() -> (memMiB: Int, cpu: Int) {
        guard let obj = JSONFile.loadDict(config.launchSpecPath),
              let res = obj["resources"] as? [String: Any] else { return (0, 0) }
        let mem = Int((res["memory"] as? String) ?? "0") ?? 0
        let cpu = Int((res["cpu"] as? String) ?? "0") ?? 0
        return (mem, cpu)
    }

    func setResources(memMiB: Int, cpu: Int) -> Bool {
        guard VMResourceLimits.contains(memoryMiB: memMiB, cpu: cpu) else { return false }
        guard var obj = JSONFile.loadDict(config.launchSpecPath),
              var res = obj["resources"] as? [String: Any] else { return false }
        res["memory"] = String(memMiB)
        res["cpu"] = String(cpu)
        res["rationale"] = "Set from BridgeVM Control app."
        obj["resources"] = res
        return JSONFile.writeDict(obj, to: config.launchSpecPath)
    }

    // Reuse one warm SSH connection across guest commands: after the first, each
    // terminal/install command skips the TCP+auth handshake (felt as near-instant
    // repeats). ControlPath is keyed by (vm, ip) so a guest reboot onto a new DHCP
    // lease transparently gets a fresh master instead of reusing a dead one.
    private func controlPath(ip: String) -> String {
        "/tmp/bvm-ssh-\(config.slug)-\(ip.replacingOccurrences(of: ".", with: "-")).sock"
    }

    private func sshBaseOpts(controlPath: String) -> [String] {
        ["-o", "ControlPath=\(controlPath)",
         "-o", "StrictHostKeyChecking=no",
         "-o", "UserKnownHostsFile=/dev/null",
         "-o", "BatchMode=yes",
         "-o", "ConnectTimeout=8",
         "-o", "LogLevel=ERROR"]
    }

    /// Make sure a warm master connection is live. Hang-safe by construction: the
    /// liveness probe (`-O check`) is a local socket op, and the master itself is
    /// launched fully detached (no pipe for `Shell.run` to block on). Command
    /// connections never create a persistent master, so they always EOF on exit.
    private func ensureMaster(ip: String, controlPath: String) {
        let host = "\(config.sshUser)@\(ip)"
        let live = Shell.run("/usr/bin/ssh", ["-O", "check"] + sshBaseOpts(controlPath: controlPath) + [host], timeout: 4)
        if live.code == 0 { return }
        let args = ["-fN", "-o", "ControlPath=\(controlPath)",
                    "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
                    "-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "LogLevel=ERROR",
                    "-o", "ControlMaster=auto", "-o", "ControlPersist=180",
                    "-i", config.sshKeyPath, host]
        // rm -f clears a stale socket left by a dead master before binding a new one.
        Shell.launchDetached(
            "\(Shell.shellCommand("/bin/rm", ["-f", controlPath])); "
                + "\(Shell.shellCommand("/usr/bin/ssh", args)) >/dev/null 2>&1"
        )
    }

    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        guard hasValidSSHUser else { return ("SSH 사용자 이름이 안전한 형식이 아닙니다.", -1) }
        guard let ip = currentIP() else { return ("게스트 IP를 찾을 수 없음 (VM이 실행 중인가요?)", -1) }
        let cp = controlPath(ip: ip)
        ensureMaster(ip: ip, controlPath: cp)
        let args = ["-i", config.sshKeyPath] + sshBaseOpts(controlPath: cp) + ["\(config.sshUser)@\(ip)", command]
        return Shell.run("/usr/bin/ssh", args, timeout: 120)
    }
}

// MARK: - QEMU Compatibility-mode backend (Windows 11 ARM)

/// QEMU + HVF for Windows 11 ARM. Replicates the verified bridgevm-qemu device
/// shape: edk2 firmware, swtpm TPM 2.0, ramfb GOP, qemu-xhci USB HID, and the
/// installer ISO as a bootable USB CD-ROM, in a native cocoa window.
final class QemuCompatBackend: VMBackend {
    let config: VMConfig
    init(_ config: VMConfig) { self.config = config }

    var displayName: String { config.displayName }
    let kind = "qemu-compat"
    let supportsGuestCommands = false
    let supportsPackageInstall = false
    let supportsClipboard = false
    let supportsSSH = false
    let supportsResourceChanges = false

    private let qemu = "/opt/homebrew/bin/qemu-system-aarch64"
    private let edk2 = "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
    private let swtpm = "/opt/homebrew/bin/swtpm"
    private var diskPath: String { config.diskPath ?? (config.bundlePath + "/disks/win.qcow2") }
    private var swtpmSock: String { config.bundlePath + "/metadata/swtpm.sock" }
    private var swtpmState: String { config.bundlePath + "/metadata/swtpm-state" }
    private func qemuOptionValue(_ value: String) -> String {
        // QEMU key-value options use commas as separators; doubled commas mean a
        // literal comma inside a path.
        value.replacingOccurrences(of: ",", with: ",,")
    }

    func isRunning() -> Bool { Shell.isProcessRunning(matching: diskPath) }
    func currentIP() -> String? { isRunning() ? "NAT (QEMU)" : nil }

    @discardableResult func start() -> Bool {
        let mem = config.memMiB ?? 6144
        let cpu = config.cpuCount ?? 4
        guard mem > 0, cpu > 0,
              FileManager.default.isExecutableFile(atPath: qemu),
              FileManager.default.isExecutableFile(atPath: swtpm),
              FileManager.default.isReadableFile(atPath: edk2),
              FileManager.default.isReadableFile(atPath: diskPath) else { return false }
        if let iso = config.isoPath, !iso.isEmpty {
            guard FileManager.default.isReadableFile(atPath: iso) else { return false }
        }
        return Shell.launchDetached(launchCommand())
    }

    func launchCommand() -> String {
        let mem = config.memMiB ?? 6144
        let cpu = config.cpuCount ?? 4
        var qemuArgs = ["-name", config.name, "-machine", "virt", "-accel", "hvf", "-cpu", "host",
                        "-m", String(mem), "-smp", String(cpu), "-bios", edk2,
                        "-device", "ramfb", "-device", "qemu-xhci,id=usb",
                        "-device", "usb-kbd,bus=usb.0", "-device", "usb-tablet,bus=usb.0",
                        "-drive", "if=none,id=disk,format=qcow2,file=\(qemuOptionValue(diskPath))",
                        "-device", "nvme,drive=disk,serial=bridgevm", "-netdev", "user,id=net0",
                        "-device", "virtio-net-pci,netdev=net0",
                        "-chardev", "socket,id=chrtpm,path=\(qemuOptionValue(swtpmSock))",
                        "-tpmdev", "emulator,id=tpm0,chardev=chrtpm",
                        "-device", "tpm-tis-device,tpmdev=tpm0", "-display", "cocoa"]
        if let iso = config.isoPath, !iso.isEmpty {
            qemuArgs += ["-drive", "if=none,id=installer,file=\(qemuOptionValue(iso)),media=cdrom,readonly=on",
                         "-device", "usb-storage,bus=usb.0,drive=installer,bootindex=0"]
        }
        let qemuCommand = "nohup \(Shell.shellCommand(qemu, qemuArgs)) >\(Shell.shQuote(config.bundlePath + "/logs/qemu.log")) 2>&1 &"
        // swtpm must listen before QEMU connects → start it, brief wait, then QEMU.
        let mkdir = Shell.shellCommand("/bin/mkdir", ["-p", swtpmState, config.bundlePath + "/logs"])
        let probe = Shell.shellCommand("/usr/bin/pgrep", ["-f", Shell.eregEscape(swtpmSock)])
        let swtpmArgs = ["socket", "--tpmstate", "dir=\(swtpmState)",
                         "--ctrl", "type=unixio,path=\(swtpmSock)", "--tpm2"]
        let startTPM = "nohup \(Shell.shellCommand(swtpm, swtpmArgs)) >/dev/null 2>&1 &"
        return "\(mkdir); \(probe) >/dev/null 2>&1 || (\(startTPM)); sleep 1.5; \(qemuCommand)"
    }

    func stop() {
        Shell.killProcesses(matching: diskPath)
        Shell.killProcesses(matching: swtpmSock)
    }
    func resources() -> (memMiB: Int, cpu: Int) { (config.memMiB ?? 0, config.cpuCount ?? 0) }
    func setResources(memMiB: Int, cpu: Int) -> Bool { false }
    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        ("Windows 게스트는 SSH 제어를 지원하지 않습니다 (QEMU).", -1)
    }
}

// MARK: - Native HVF engine backend (Windows 11 ARM)

final class HvfWindowsBackend: VMBackend {
    private(set) var config: VMConfig
    private let processIsRunning: (String) -> Bool
    private let libraryRoot: URL
    init(
        _ config: VMConfig,
        processIsRunning: @escaping (String) -> Bool = { Shell.isProcessRunning(matching: $0) },
        libraryRoot: URL = VMLibrary.root
    ) {
        self.config = config
        self.processIsRunning = processIsRunning
        self.libraryRoot = libraryRoot
    }

    var displayName: String { config.displayName }
    let kind = "hvf-engine"
    let supportsGuestCommands = true
    let supportsPackageInstall = false
    let supportsClipboard = true
    let supportsSSH = false
    let supportsResourceChanges = true

    var targetDiskPath: String { config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw") }
    var uefiVarsPath: String { config.bundlePath + "/metadata/hvf-vars.fd" }
    var evidenceDir: String { config.bundlePath + "/logs/hvf" }
    var ctlFilePath: String { config.bundlePath + "/metadata/hvf.ctl" }
    var repoRoot: URL { HvfEngineSession.defaultRepoRoot() }

    private var wrapperName: String { "scripts/run-hvf-windows-installed-boot.sh" }
    private var runLogPath: String { evidenceDir + "/run.log" }
    private var diskGrowMarkerPath: String { config.bundlePath + "/metadata/hvf-grow-pending" }
    private let ctlWriteLock = NSLock()
    private let guestCommandLock = NSLock()
    private let serviceStartReader = HvfIncrementalMarkerReader(marker: "BVAGENT SERVICE start")
    var launcherLogPath: String { evidenceDir + "/launcher.log" }

    static let pendingDiskGrowthCommand = [
        "powershell.exe -NoLogo -NoProfile -NonInteractive -Command \"$ErrorActionPreference='Stop';",
        "Update-HostStorageCache;",
        "$before=Get-Partition -DriveLetter C;",
        "$supported=Get-PartitionSupportedSize -DriveLetter C;",
        "$state='resized';",
        "if($supported.SizeMax -le $before.Size){",
        "$disk=Get-Disk -Number $before.DiskNumber;",
        "$partitionEnd=[UInt64]$before.Offset+[UInt64]$before.Size;",
        "if($partitionEnd -gt [UInt64]$disk.Size){throw 'C: partition extends beyond its disk'};",
        "$tailGap=[UInt64]$disk.Size-$partitionEnd;",
        "if($tailGap -gt 16777216){throw 'C: has no contiguous extension space'};",
        "$state='already-max';$after=$before;",
        "}else{",
        "Resize-Partition -DriveLetter C -Size $supported.SizeMax;",
        "$after=Get-Partition -DriveLetter C;",
        "if($after.Size -le $before.Size){throw 'C: partition size did not increase'}",
        "};",
        "$volume=Get-Volume -DriveLetter C;",
        "Write-Output ('BRIDGEVM_DISK_GROW_OK state='+$state+' size='+$after.Size+' free='+$volume.SizeRemaining)\""
    ].joined()

    func isRunning() -> Bool { processIsRunning(targetDiskPath) }
    func currentIP() -> String? { isRunning() ? "NAT (HVF)" : nil }

    @discardableResult func start() -> Bool {
        guard !isRunning() else { return true }
        guard ensureDirectories() else { return false }
        guard ensureControlFile() else { return false }
        let wrapper = repoRoot.appendingPathComponent(wrapperName)
        guard FileManager.default.isExecutableFile(atPath: wrapper.path) else { return false }
        // The wrapper replaces run.log on every launch. Remove it first so a
        // pending first-boot action cannot mistake the previous SERVICE marker
        // for the new guest generation and append a command before tailing starts.
        if FileManager.default.fileExists(atPath: runLogPath) {
            do {
                try FileManager.default.removeItem(atPath: runLogPath)
            } catch {
                return false
            }
        }
        serviceStartReader.reset()
        guard Shell.launchDetached(launchCommand()) else { return false }
        schedulePendingDiskGrowth()
        return true
    }

    func launchCommand() -> String {
        let args = makeHvfEngineConfig().wrapperArguments().map(Shell.shQuote).joined(separator: " ")
        return "cd \(Shell.shQuote(repoRoot.path)) && nohup /usr/bin/env \(args) >\(Shell.shQuote(launcherLogPath)) 2>&1 &"
    }

    func stop() {
        guard isRunning() else { return }
        let serviceDeadline = Date().addingTimeInterval(180)
        while isRunning(), !serviceHasStarted(), Date() < serviceDeadline {
            usleep(250_000)
        }
        if isRunning(), serviceHasStarted() {
            if requestGracefulStop() {
                // Give the guest a complete grace period after the request. Reusing
                // the service-discovery deadline could leave only milliseconds when
                // READY arrived late and turn a valid clean shutdown into a kill.
                let shutdownDeadline = Date().addingTimeInterval(180)
                while isRunning(), Date() < shutdownDeadline {
                    usleep(500_000)
                }
            }
        }
        guard isRunning() else { return }
        Shell.killProcesses(matching: targetDiskPath)
        Shell.killProcesses(matching: "\(wrapperName) --target \(targetDiskPath)")
    }

    @discardableResult
    func requestGracefulStop() -> Bool {
        guard ensureDirectories() else { return false }
        return appendCtl("shutdown.exe /p /f")
    }

    func resources() -> (memMiB: Int, cpu: Int) {
        (config.memMiB ?? 4096, config.cpuCount ?? 1)
    }

    func setResources(memMiB: Int, cpu: Int) -> Bool {
        guard VMResourceLimits.contains(memoryMiB: memMiB, cpu: cpu) else { return false }
        var updated = config
        updated.memMiB = memMiB
        updated.cpuCount = cpu
        guard VMLibrary.save(updated, rootURL: libraryRoot) else { return false }
        config = updated
        return true
    }

    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        executeGuestCommand(command, timeout: 900)
    }

    private func executeGuestCommand(_ command: String, timeout: TimeInterval) -> (output: String, code: Int32) {
        let normalized: String
        switch HvfGuestCommand.normalize(command) {
        case let .success(value):
            normalized = value
        case let .failure(error):
            return (error.message, -1)
        }
        guestCommandLock.lock()
        defer { guestCommandLock.unlock() }
        guard isRunning() else {
            return ("HVF VM이 실행 중이 아닙니다.", -1)
        }
        guard ensureDirectories() else {
            return ("HVF 런타임 디렉터리를 준비하지 못했습니다: \(config.bundlePath)", -1)
        }
        let offset = fileSize(at: runLogPath)
        guard appendCtl(normalized) else {
            return ("HVF 제어 채널에 명령을 기록하지 못했습니다: \(ctlFilePath)", -1)
        }
        // The guest dispatcher is deliberately lockstep. Driver/tool installs
        // can take minutes, and returning a false timeout while their reply is
        // still in flight invites the next UI command to be misinterpreted.
        return waitForCommandReply(command: normalized, offset: offset, timeout: timeout)
    }

    private func makeHvfEngineConfig() -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: targetDiskPath,
                        uefiVarsPath: uefiVarsPath,
                        evidenceDir: evidenceDir,
                        watchdogMs: nil,
                        ramMiB: config.memMiB ?? 6144,
                        smpCpus: config.cpuCount ?? 4,
                        clipboardSync: true,
                        shareHostDir: nil,
                        shareGuestDir: nil,
                        virtioNet: true,
                        virtioGpu3d: true,
                        nvmeBufferedIO: true,
                        ctlFilePath: ctlFilePath)
    }

    @discardableResult
    private func ensureDirectories() -> Bool {
        for path in [
            config.bundlePath + "/disks",
            config.bundlePath + "/metadata",
            config.bundlePath + "/logs",
            evidenceDir
        ] {
            do {
                try FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
            } catch {
                return false
            }
        }
        return true
    }

    private func ensureControlFile() -> Bool {
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: ctlFilePath) {
            guard let attributes = try? fileManager.attributesOfItem(atPath: ctlFilePath) else {
                return false
            }
            return attributes[.type] as? FileAttributeType == .typeRegular
        }
        return fileManager.createFile(atPath: ctlFilePath, contents: nil)
    }

    private func serviceHasStarted() -> Bool {
        serviceStartReader.containsMarker(in: URL(fileURLWithPath: runLogPath))
    }

    private func schedulePendingDiskGrowth() {
        guard FileManager.default.fileExists(atPath: diskGrowMarkerPath) else { return }
        DispatchQueue.global(qos: .utility).async { [weak self] in
            guard let self else { return }
            let deadline = Date().addingTimeInterval(300)
            while Date() < deadline {
                if self.isRunning(), self.serviceHasStarted() { break }
                usleep(250_000)
            }
            guard self.isRunning(), self.serviceHasStarted() else { return }
            let command = Self.pendingDiskGrowthCommand
            let reply = self.executeGuestCommand(command, timeout: 300)
            if reply.code == 0, reply.output.contains("BRIDGEVM_DISK_GROW_OK") {
                try? FileManager.default.removeItem(atPath: self.diskGrowMarkerPath)
            }
        }
    }

    @discardableResult
    private func appendCtl(_ command: String) -> Bool {
        let cleaned = command.trimmingCharacters(in: .newlines)
        guard !cleaned.isEmpty, let data = "\(cleaned)\n".data(using: .utf8) else { return false }
        ctlWriteLock.lock()
        defer { ctlWriteLock.unlock() }
        if !FileManager.default.fileExists(atPath: ctlFilePath) {
            guard FileManager.default.createFile(atPath: ctlFilePath, contents: nil) else {
                return false
            }
        }
        do {
            let handle = try FileHandle(forWritingTo: URL(fileURLWithPath: ctlFilePath))
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: data)
            return true
        } catch {
            return false
        }
    }

    private func waitForCommandReply(command: String, offset: UInt64, timeout: TimeInterval) -> (output: String, code: Int32) {
        let deadline = Date().addingTimeInterval(timeout)
        let reader = HvfCommandReplyReader(command: command, offset: offset)
        let logURL = URL(fileURLWithPath: runLogPath)
        while Date() < deadline {
            if let reply = reader.readReply(from: logURL) {
                return reply
            }
            if !isRunning() {
                return ("HVF 게스트 연결이 명령 실행 중 종료되었습니다: \(command)", -1)
            }
            usleep(100_000)
        }
        return ("HVF 게스트 명령 응답 시간 초과: \(command)", -1)
    }

    private func fileSize(at path: String) -> UInt64 {
        ((try? FileManager.default.attributesOfItem(atPath: path)[.size] as? NSNumber)?.uint64Value) ?? 0
    }

}

// MARK: - JSON file helper

enum JSONFile {
    static let maximumBytes = 4 * 1_048_576

    static func loadDict(_ path: String) -> [String: Any]? {
        let url = URL(fileURLWithPath: path).standardizedFileURL
        guard hasSafeFileAndParent(url) else { return nil }
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true,
              let size = values.fileSize,
              size <= maximumBytes,
              let data = try? Data(contentsOf: url, options: [.mappedIfSafe]) else { return nil }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }

    @discardableResult
    static func writeDict(_ obj: [String: Any], to path: String) -> Bool {
        guard let data = try? JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted]) else { return false }
        guard data.count <= maximumBytes else { return false }
        let url = URL(fileURLWithPath: path).standardizedFileURL
        guard hasSafeFileAndParent(url, allowMissingFile: true) else { return false }
        return (try? data.write(to: url, options: [.atomic])) != nil
    }

    private static func hasSafeFileAndParent(_ url: URL, allowMissingFile: Bool = false) -> Bool {
        let parent = url.deletingLastPathComponent()
        if (try? parent.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
            return false
        }
        if (try? url.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
            return false
        }
        return allowMissingFile || FileManager.default.fileExists(atPath: url.path)
    }
}
