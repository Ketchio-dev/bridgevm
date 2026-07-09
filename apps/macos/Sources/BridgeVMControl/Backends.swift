import Foundation

// MARK: - Configuration

/// One controllable VM. Persisted per-VM as vm.json inside the library so the
/// app manages MANY VMs. `id`/`bootMode` are optional for backward-compat with
/// the legacy single ~/.bridgevm-control/config.json.
struct VMConfig: Codable, Identifiable {
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

    var slug: String { id ?? VMConfig.slugify(name) }
    var effectiveBootMode: String { bootMode ?? "direct-kernel" }
    var engineKind: BackendKind { BackendKind(rawValue: backendKind) ?? .fastVZ }
    var engineShortLabel: String { engineKind.shortLabel }
    var engineDetailLabel: String { engineKind.detailLabel }

    static func slugify(_ s: String) -> String {
        var out = ""
        for ch in s.lowercased() {
            if ch.isLetter || ch.isNumber { out.append(ch) }
            else if ch == " " || ch == "-" || ch == "_" || ch == "." { out.append("-") }
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
    /// Run a command to completion (bounded by `timeout`) and return its combined
    /// stdout+stderr and exit code. Output is drained on a background queue so a
    /// child that writes more than the pipe buffer can never block/deadlock.
    @discardableResult
    static func run(_ launchPath: String, _ args: [String], timeout: Double = 30) -> (output: String, code: Int32) {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: launchPath)
        proc.arguments = args
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = pipe

        var collected = Data()
        let group = DispatchGroup()
        group.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            collected = pipe.fileHandleForReading.readDataToEndOfFile()  // blocks until EOF (child exit)
            group.leave()
        }
        do {
            try proc.run()
        } catch {
            return ("실행 실패: \(error.localizedDescription)", -1)
        }
        let deadline = Date().addingTimeInterval(timeout)
        while proc.isRunning && Date() < deadline { usleep(50_000) }
        if proc.isRunning {
            proc.terminate()
            usleep(300_000)
            if proc.isRunning { kill(proc.processIdentifier, SIGKILL) }
        }
        proc.waitUntilExit()
        _ = group.wait(timeout: .now() + 2)   // let the reader hit EOF after the pipe's write end closes
        return (String(data: collected, encoding: .utf8) ?? "", proc.terminationStatus)
    }

    /// Launch a long-running process fully detached from this app (its own window).
    static func launchDetached(_ command: String) {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/sh")
        proc.arguments = ["-c", command]
        try? proc.run()
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
    func start()
    func stop()
    func resources() -> (memMiB: Int, cpu: Int)
    func setResources(memMiB: Int, cpu: Int) -> Bool
    /// Run a shell command inside the guest, return its combined output.
    func runInGuest(_ command: String) -> (output: String, code: Int32)
    /// True when a control channel into the guest (e.g. SSH) is available — gates
    /// the in-app terminal and software-install UI.
    var supportsGuestCommands: Bool { get }
}

// MARK: - Fast Mode (Apple Virtualization.framework) backend

final class FastVZBackend: VMBackend {
    let config: VMConfig
    init(_ config: VMConfig) { self.config = config }

    var displayName: String { config.displayName }
    let kind = "fast-vz"
    let supportsGuestCommands = true

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
            if matched, line.hasPrefix("ip_address=") { return String(line.dropFirst("ip_address=".count)) }
        }
        return nil
    }

    func start() {
        // Launch the signed AppleVzRunner with its own display window, detached.
        let cmd = "BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 nohup \"\(config.runnerPath)\" "
            + "--display --display-width \(config.displayWidth) --display-height \(config.displayHeight) "
            + "--allow-real-vz-start --handoff-json \"\(config.handoffPath)\" "
            + ">/dev/null 2>&1 &"
        Shell.launchDetached(cmd)
    }

    func resources() -> (memMiB: Int, cpu: Int) {
        guard let obj = JSONFile.loadDict(config.launchSpecPath),
              let res = obj["resources"] as? [String: Any] else { return (0, 0) }
        let mem = Int((res["memory"] as? String) ?? "0") ?? 0
        let cpu = Int((res["cpu"] as? String) ?? "0") ?? 0
        return (mem, cpu)
    }

    func setResources(memMiB: Int, cpu: Int) -> Bool {
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
        let opts = "-o ControlPath=\"\(controlPath)\" -o StrictHostKeyChecking=no "
            + "-o UserKnownHostsFile=/dev/null -o BatchMode=yes -o ConnectTimeout=8 "
            + "-o LogLevel=ERROR -o ControlMaster=auto -o ControlPersist=180"
        // rm -f clears a stale socket left by a dead master before binding a new one.
        Shell.launchDetached("rm -f \"\(controlPath)\"; /usr/bin/ssh -fN \(opts) -i \"\(config.sshKeyPath)\" \(host) >/dev/null 2>&1")
    }

    func runInGuest(_ command: String) -> (output: String, code: Int32) {
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

    private let qemu = "/opt/homebrew/bin/qemu-system-aarch64"
    private let edk2 = "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
    private var diskPath: String { config.diskPath ?? (config.bundlePath + "/disks/win.qcow2") }
    private var swtpmSock: String { config.bundlePath + "/metadata/swtpm.sock" }
    private var swtpmState: String { config.bundlePath + "/metadata/swtpm-state" }

    func isRunning() -> Bool { Shell.isProcessRunning(matching: diskPath) }
    func currentIP() -> String? { isRunning() ? "NAT (QEMU)" : nil }

    func start() {
        let mem = config.memMiB ?? 6144
        let cpu = config.cpuCount ?? 4
        var q = "nohup \"\(qemu)\" -name \"\(config.name)\" -machine virt -accel hvf -cpu host "
            + "-m \(mem) -smp \(cpu) -bios \"\(edk2)\" "
            + "-device ramfb -device qemu-xhci,id=usb -device usb-kbd,bus=usb.0 -device usb-tablet,bus=usb.0 "
            + "-drive if=none,id=disk,format=qcow2,file=\"\(diskPath)\" -device nvme,drive=disk,serial=bridgevm "
            + "-netdev user,id=net0 -device virtio-net-pci,netdev=net0 "
            + "-chardev socket,id=chrtpm,path=\"\(swtpmSock)\" -tpmdev emulator,id=tpm0,chardev=chrtpm -device tpm-tis-device,tpmdev=tpm0 "
            + "-display cocoa "
        if let iso = config.isoPath, !iso.isEmpty {
            q += "-drive if=none,id=installer,file=\"\(iso)\",media=cdrom,readonly=on "
                + "-device usb-storage,bus=usb.0,drive=installer,bootindex=0 "
        }
        q += ">\"\(config.bundlePath)/logs/qemu.log\" 2>&1 &"
        // swtpm must listen before QEMU connects → start it, brief wait, then QEMU.
        let script = "mkdir -p \"\(swtpmState)\" \"\(config.bundlePath)/logs\"; "
            + "pgrep -f \"\(swtpmSock)\" >/dev/null 2>&1 || "
            + "(nohup swtpm socket --tpmstate dir=\"\(swtpmState)\" --ctrl type=unixio,path=\"\(swtpmSock)\" --tpm2 >/dev/null 2>&1 &); "
            + "sleep 1.5; " + q
        Shell.launchDetached(script)
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
    init(_ config: VMConfig) { self.config = config }

    var displayName: String { config.displayName }
    let kind = "hvf-engine"
    let supportsGuestCommands = true

    var targetDiskPath: String { config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw") }
    var uefiVarsPath: String { config.bundlePath + "/metadata/hvf-vars.fd" }
    var evidenceDir: String { config.bundlePath + "/logs/hvf" }
    var ctlFilePath: String { config.bundlePath + "/metadata/hvf.ctl" }
    var repoRoot: URL { HvfEngineSession.defaultRepoRoot() }

    private var wrapperName: String { "scripts/run-hvf-windows-installed-boot.sh" }
    private var runLogPath: String { evidenceDir + "/run.log" }

    func isRunning() -> Bool { Shell.isProcessRunning(matching: targetDiskPath) }
    func currentIP() -> String? { isRunning() ? "NAT (HVF)" : nil }

    func start() {
        ensureDirectories()
        let hvfConfig = makeHvfEngineConfig()
        let env = hvfConfig.environment()
            .sorted { $0.key < $1.key }
            .map { "\($0.key)=\(Shell.shQuote($0.value))" }
            .joined(separator: " ")
        let args = hvfConfig.wrapperArguments().map(Shell.shQuote).joined(separator: " ")
        let cmd = "cd \(Shell.shQuote(repoRoot.path)) && \(env) nohup /usr/bin/env \(args) >\(Shell.shQuote(runLogPath)) 2>&1 &"
        Shell.launchDetached(cmd)
    }

    func stop() {
        Shell.killProcesses(matching: targetDiskPath)
        Shell.killProcesses(matching: "\(wrapperName) --target \(targetDiskPath)")
    }

    func resources() -> (memMiB: Int, cpu: Int) {
        (config.memMiB ?? 4096, config.cpuCount ?? 1)
    }

    func setResources(memMiB: Int, cpu: Int) -> Bool {
        config.memMiB = memMiB
        config.cpuCount = cpu
        VMLibrary.save(config)
        return true
    }

    func runInGuest(_ command: String) -> (output: String, code: Int32) {
        ensureDirectories()
        let offset = fileSize(at: runLogPath)
        appendCtl(command)
        return waitForCommandReply(command: command, offset: offset, timeout: 15)
    }

    private func makeHvfEngineConfig() -> HvfEngineConfig {
        HvfEngineConfig(targetDiskPath: targetDiskPath,
                        uefiVarsPath: uefiVarsPath,
                        evidenceDir: evidenceDir,
                        watchdogMs: 900_000,
                        clipboardSync: true,
                        shareHostDir: nil,
                        shareGuestDir: nil,
                        virtioNet: true,
                        ctlFilePath: ctlFilePath)
    }

    private func ensureDirectories() {
        for path in [
            config.bundlePath + "/disks",
            config.bundlePath + "/metadata",
            config.bundlePath + "/logs",
            evidenceDir
        ] {
            try? FileManager.default.createDirectory(atPath: path, withIntermediateDirectories: true)
        }
    }

    private func appendCtl(_ command: String) {
        let cleaned = command.trimmingCharacters(in: .newlines)
        guard !cleaned.isEmpty else { return }
        if !FileManager.default.fileExists(atPath: ctlFilePath) {
            FileManager.default.createFile(atPath: ctlFilePath, contents: nil)
        }
        guard let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: ctlFilePath)) else { return }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        if let data = "\(cleaned)\n".data(using: .utf8) {
            try? handle.write(contentsOf: data)
        }
    }

    private func waitForCommandReply(command: String, offset: UInt64, timeout: TimeInterval) -> (output: String, code: Int32) {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let reply = parseCommandReply(command: command, from: logSlice(startingAt: offset)) {
                return reply
            }
            usleep(100_000)
        }
        return ("HVF 게스트 명령 응답 시간 초과: \(command)", -1)
    }

    private func parseCommandReply(command: String, from text: String) -> (output: String, code: Int32)? {
        let lines = text.components(separatedBy: .newlines)
        var collecting = false
        var body: [String] = []
        var exitCode: Int32 = -1
        let startPrefix = "BVAGENT CMD \(command) exit="
        let endLine = "BVAGENT END \(command)"

        for line in lines {
            if collecting {
                if line == endLine {
                    return (body.joined(separator: "\n"), exitCode)
                }
                body.append(line)
            } else if line.hasPrefix(startPrefix) {
                let rawCode = line.dropFirst(startPrefix.count).prefix { $0 == "-" || $0.isNumber }
                exitCode = Int32(String(rawCode)) ?? -1
                collecting = true
            }
        }
        return nil
    }

    private func fileSize(at path: String) -> UInt64 {
        ((try? FileManager.default.attributesOfItem(atPath: path)[.size] as? NSNumber)?.uint64Value) ?? 0
    }

    private func logSlice(startingAt offset: UInt64) -> String {
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: runLogPath)) else { return "" }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: min(offset, try handle.seekToEnd()))
            let data = handle.readDataToEndOfFile()
            return String(data: data, encoding: .utf8) ?? ""
        } catch {
            return ""
        }
    }
}

// MARK: - JSON file helper

enum JSONFile {
    static func loadDict(_ path: String) -> [String: Any]? {
        guard let data = FileManager.default.contents(atPath: path) else { return nil }
        return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    }
    @discardableResult
    static func writeDict(_ obj: [String: Any], to path: String) -> Bool {
        guard let data = try? JSONSerialization.data(withJSONObject: obj, options: [.prettyPrinted]) else { return false }
        return (try? data.write(to: URL(fileURLWithPath: path), options: [.atomic])) != nil
    }
}
