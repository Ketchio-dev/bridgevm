import Foundation

final class HvfWindowsBackend: VMBackend {
    private(set) var config: VMConfig
    private let processIsRunning: (String) -> Bool
    private let libraryRoot: URL
    private let vtpmKeyProvider: VTPMStateKeyProviding
    private(set) var launchedProcess: Process?
    init(
        _ config: VMConfig,
        processIsRunning: @escaping (String) -> Bool = { Shell.isProcessRunning(matching: $0) },
        libraryRoot: URL = VMLibrary.root,
        vtpmKeyProvider: VTPMStateKeyProviding = KeychainVTPMStateKeyStore()
    ) {
        self.config = config
        self.processIsRunning = processIsRunning
        self.libraryRoot = libraryRoot
        self.vtpmKeyProvider = vtpmKeyProvider
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
        guard HvfPackagedWrapperPolicy.wrapperAvailable(repoRoot: repoRoot) && HvfPackagedWrapperPolicy.signatureVerified(repoRoot: repoRoot) else { return false }
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
        let engineConfig = makeHvfEngineConfig()
        guard engineConfig.readiness(repoRoot: repoRoot).launchReady else { return false }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = engineConfig.wrapperArguments()
        process.currentDirectoryURL = repoRoot
        process.environment = HvfPackagedWrapperPolicy.environment(ProcessInfo.processInfo.environment)
        var keyInput: VTPMProcessKeyInput? = nil
        do {
            keyInput = try VTPMStateSecurity.processInput(
                for: engineConfig,
                provider: vtpmKeyProvider
            )
            keyInput?.attach(to: process)
            if !FileManager.default.fileExists(atPath: launcherLogPath) {
                FileManager.default.createFile(atPath: launcherLogPath, contents: nil)
            }
            let launcherLog = try FileHandle(forWritingTo: URL(fileURLWithPath: launcherLogPath))
            defer { try? launcherLog.close() }
            try launcherLog.seekToEnd()
            process.standardOutput = launcherLog
            process.standardError = launcherLog
            try process.run()
            do {
                try keyInput?.deliverAfterLaunch()
            } catch {
                process.terminate()
                return false
            }
        } catch {
            keyInput?.discard()
            return false
        }
        launchedProcess = process
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
        let launch = makeHvfEngineConfig()
        return (launch.ramMiB, launch.smpCpus)
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

    func makeHvfEngineConfig() -> HvfEngineConfig {
        HvfEngineConfig.libraryVM(config, rootURL: libraryRoot) ?? HvfEngineConfig(
            targetDiskPath: targetDiskPath,
            uefiVarsPath: uefiVarsPath,
            evidenceDir: evidenceDir,
            watchdogMs: nil,
            ramMiB: config.memMiB ?? 6144,
            smpCpus: config.cpuCount ?? 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: config.networkEnabled ?? true,
            virtioGpu3d: config.experimental3DAllowed ?? false,
            nvmeBufferedIO: false,
            ctlFilePath: ctlFilePath, allowsExperimental3D: config.experimental3DAllowed ?? false
        )
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
