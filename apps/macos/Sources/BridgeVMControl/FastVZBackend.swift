import Foundation

// MARK: - Fast Mode (Apple Virtualization.framework) backend

final class FastVZBackend: VMOwnedLaunchingBackend {
    private(set) var config: VMConfig
    private let libraryRoot: URL
    init(_ config: VMConfig, libraryRoot: URL = VMLibrary.root) { self.config = config; self.libraryRoot = libraryRoot }

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
        FastVZLaunchDefinition(config: config).command
    }

    func ownedLaunchPlan() -> GenericOwnedLaunchPlan? {
        FastVZLaunchDefinition(config: config).validatedPlan()
    }

    func resources() -> (memMiB: Int, cpu: Int) {
        guard let obj = JSONFile.loadDict(config.launchSpecPath),
              let res = obj["resources"] as? [String: Any] else { return (0, 0) }
        let mem = Int((res["memory"] as? String) ?? "0") ?? 0
        let cpu = Int((res["cpu"] as? String) ?? "0") ?? 0
        return (mem, cpu)
    }

    /// Launch spec, handoff and vm.json together; see FastVZResourceUpdate for ordering.
    func setResources(memMiB: Int, cpu: Int) -> Bool {
        guard let updated = FastVZResourceUpdate.apply(config, memMiB: memMiB, cpu: cpu, libraryRoot: libraryRoot) else { return false }
        config = updated
        return true
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
