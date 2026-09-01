import Foundation
import Combine

// MARK: - Install request (persisted per VM bundle)

/// A pending Windows HVF install described at VM-creation time and executed
/// later from the VM detail panel. Persisted as metadata/hvf-install.json so
/// an interrupted install can be retried after an app relaunch.
struct HvfWindowsInstallRequest: Codable, Equatable {
    var isoPath: String
    var isoSHA256: String? = nil
    var diskGiB: Int
    var injectViogpu3d: Bool
    var driverPackageDir: String?

    static let fileName = "metadata/hvf-install.json"
    static let doneFileName = "metadata/hvf-install-done.json"

    static func load(bundlePath: String) -> HvfWindowsInstallRequest? {
        let url = URL(fileURLWithPath: bundlePath).appendingPathComponent(fileName)
        guard let data = FileManager.default.contents(atPath: url.path) else { return nil }
        return try? JSONDecoder().decode(HvfWindowsInstallRequest.self, from: data)
    }

    @discardableResult
    func save(bundlePath: String) -> Bool {
        let url = URL(fileURLWithPath: bundlePath).appendingPathComponent(Self.fileName)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(self) else { return false }
        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: url, options: [.atomic])
            return true
        } catch {
            return false
        }
    }
}

// MARK: - Install plan (pure path/argument computation, unit-testable)

struct HvfWindowsInstallPlan: Equatable {
    let repoRoot: URL
    let bundlePath: String
    let slug: String
    let request: HvfWindowsInstallRequest
    let sealedISOSHA256: String
    let sourceCacheKey: String
    var homeDirectory: String = NSHomeDirectory()

    init(repoRoot: URL, bundlePath: String, slug: String, request: HvfWindowsInstallRequest) {
        self.repoRoot = repoRoot
        self.bundlePath = bundlePath
        self.slug = slug
        self.request = request
        sealedISOSHA256 = request.isoSHA256
            ?? HvfWindowsInstallCacheIdentity.sha256File(request.isoPath) ?? "absent"
        sourceCacheKey = HvfWindowsInstallCacheIdentity.key(
            isoSHA256: sealedISOSHA256, repoRoot: repoRoot)
    }

    static let minimumDiskGiB = 64
    static var wimlibCandidates: [String] {
        ["/opt/homebrew/bin/wimlib-imagex", "/usr/local/bin/wimlib-imagex"]
    }

    static let installResourcePaths = [
        "scripts/build-hvf-windows-scripted-source.sh",
        "scripts/hvf-disk-image-utils.sh",
        "scripts/run-hvf-windows-scripted-install.sh",
        "target/release/examples/hvf_gic_boot_probe",
        "scripts/win-assets/winpeshl.ini",
        "scripts/win-assets/bvinstall.cmd",
        "scripts/win-assets/bvdiskpart.txt",
    ]

    var sourceImagePath: String { "\(homeDirectory)/BridgeVM/bridgevm-app-src/\(sourceCacheKey).raw" }
    var wimlibPath: String? {
        Self.wimlibCandidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    var tmpTargetPath: String { "/tmp/bridgevm-appinstall-\(slug)-target.raw" }
    var tmpVarsPath: String { "/tmp/bridgevm-appinstall-\(slug)-vars.fd" }
    var tmpEvidenceDir: String { "/tmp/bridgevm-appinstall-\(slug)-evidence" }

    var bundleDiskPath: String { "\(bundlePath)/disks/hvf-target.raw" }
    var bundleVarsPath: String { "\(bundlePath)/metadata/hvf-vars.fd" }
    var bundleInstallLogPath: String { "\(bundlePath)/logs/install-run.log" }

    var freshTargetSizeBytes: UInt64 { UInt64(request.diskGiB) * 1024 * 1024 * 1024 }

    // MARK: commands

    /// Stage a: host-side WinPE scripted-installer source build from the ISO.
    func sourceBuildCommand() -> (environment: [String: String], arguments: [String]) {
        (
            environment: [
                "ISO": request.isoPath,
                "OUT": sourceImagePath,
                "WIMLIB": wimlibPath ?? "",
            ],
            arguments: ["/bin/bash", "scripts/build-hvf-windows-scripted-source.sh"]
        )
    }

    /// Stage c: the unattended scripted install boot (WIM apply + bcdboot +
    /// unattended OOBE; reboots into the installed OS before exiting).
    func installCommand() -> [String] {
        let arguments = [
            "/bin/bash", "scripts/run-hvf-windows-scripted-install.sh",
            "--source", sourceImagePath,
            "--target", tmpTargetPath,
            "--fresh-target-size", String(freshTargetSizeBytes),
            "--vars", tmpVarsPath,
            "--evidence-dir", tmpEvidenceDir,
            "--release",
            "--skip-build",
            "--watchdog-ms", "1500000",
        ]
        return arguments
    }

    // MARK: validation

    static func whitespaceFree(_ path: String) -> Bool {
        path.rangeOfCharacter(from: .whitespacesAndNewlines) == nil
    }

    static func driverPackageError(_ directory: String) -> String? {
        let fm = FileManager.default
        var isDirectory: ObjCBool = false
        guard fm.fileExists(atPath: directory, isDirectory: &isDirectory), isDirectory.boolValue else {
            return "viogpu3d 드라이버 패키지 폴더를 찾을 수 없습니다."
        }
        guard whitespaceFree(directory) else {
            return "드라이버 패키지 경로에 공백이 있으면 인젝터를 만들 수 없습니다."
        }
        let entries = (try? fm.contentsOfDirectory(atPath: directory)) ?? []
        let lowered = entries.map { $0.lowercased() }
        guard lowered.contains(where: { $0.hasSuffix(".inf") }) else {
            return "드라이버 패키지 폴더에 .inf 파일이 없습니다."
        }
        guard lowered.contains(where: { $0.hasSuffix(".sys") }) else {
            return "드라이버 패키지 폴더에 .sys 파일이 없습니다."
        }
        return HvfWindowsDriverPreflight.inspect(packageDirectory: directory).userMessage
    }

    func validationError() -> String? {
        if let blocker = VMLibrary.windowsHVFInjectionError(requested: request.injectViogpu3d) {
            return blocker
        }
        let fm = FileManager.default
        guard fm.isReadableFile(atPath: request.isoPath) else {
            return "Windows 11 ARM64 ISO 파일을 찾을 수 없습니다."
        }
        guard HvfWindowsInstallCacheIdentity.sha256File(request.isoPath) == sealedISOSHA256 else {
            return "선택한 Windows ISO가 VM 생성 후 변경되었습니다."
        }
        guard HvfWindowsInstallCacheIdentity.key(isoSHA256: sealedISOSHA256, repoRoot: repoRoot)
                == sourceCacheKey else { return "Windows 설치 레시피 또는 wimlib가 변경되었습니다." }
        let requiredResources = Self.installResourcePaths
        if let missing = requiredResources.first(where: {
            !fm.isReadableFile(atPath: repoRoot.appendingPathComponent($0).path)
        }) {
            return "앱 설치 리소스가 없습니다: \(missing)"
        }
        guard request.diskGiB >= Self.minimumDiskGiB else {
            return "디스크 크기는 최소 \(Self.minimumDiskGiB) GiB여야 합니다."
        }
        guard wimlibPath != nil else {
            return "wimlib-imagex가 필요합니다: brew install wimlib"
        }
        guard Self.whitespaceFree(sourceImagePath) else {
            return "홈 디렉터리 경로에 공백이 있으면 설치 소스를 만들 수 없습니다."
        }
        return nil
    }
}

// MARK: - Install session (stage machine + Process orchestration)

enum HvfWindowsInstallStage: Equatable {
    case idle
    case preparingSource
    case installing
    case finalizing
    case done
    case failed(String)

    var label: String {
        switch self {
        case .idle: return "대기"
        case .preparingSource: return "설치 소스 준비 (WIM 분할)"
        case .installing: return "Windows 무인 설치"
        case .finalizing: return "VM에 반영"
        case .done: return "완료"
        case .failed: return "실패"
        }
    }
}

@MainActor
final class HvfWindowsInstallSession: ObservableObject {
    @Published private(set) var stage: HvfWindowsInstallStage = .idle
    @Published private(set) var logLines: [String] = []
    @Published private(set) var startedAt: Date?

    let plan: HvfWindowsInstallPlan
    /// Called on the main actor after a successful finalize so the library can
    /// clear installPending and refresh.
    var onCompleted: (() -> Void)?

    private var currentProcess: Process?
    private var logTimer: Timer?
    private var evidenceTail = TailOffsetReader()
    private var cancelled = false

    init(plan: HvfWindowsInstallPlan) {
        self.plan = plan
    }

    var isRunning: Bool {
        switch stage {
        case .preparingSource, .installing, .finalizing: return true
        default: return false
        }
    }

    func start() {
        guard !isRunning else { return }
        if let error = plan.validationError() {
            stage = .failed(error)
            return
        }
        cancelled = false
        startedAt = Date()
        logLines = []
        evidenceTail = TailOffsetReader()
        Task { await run() }
    }

    func cancel() {
        cancelled = true
        currentProcess?.terminate()
        appendLog("사용자가 설치를 취소했습니다.")
    }

    private func run() async {
        if !(await plan.sourceImageCacheIsVerified()) {
            stage = .preparingSource
            let build = plan.sourceBuildCommand()
            guard await runProcess(arguments: build.arguments, extraEnvironment: build.environment,
                                   progressLog: nil) else {
                try? FileManager.default.removeItem(atPath: plan.sourceImagePath)
                failUnlessCancelled("설치 소스 생성이 실패했습니다.")
                return
            }
        } else {
            appendLog("설치 소스 캐시 재사용: \(plan.sourceImagePath)")
        }

        stage = .installing
        do {
            try HvfWindowsBootSeed.writeBundledSeed(to: plan.tmpVarsPath)
        } catch {
            failUnlessCancelled("번들된 UEFI vars 시드를 준비하지 못했습니다.")
            return
        }
        try? FileManager.default.createDirectory(
            atPath: plan.tmpEvidenceDir, withIntermediateDirectories: true)
        let installLog = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        guard await runProcess(arguments: plan.installCommand(), extraEnvironment: [:],
                               progressLog: installLog) else {
            failUnlessCancelled("Windows 무인 설치가 실패했습니다. 로그: \(plan.tmpEvidenceDir)/run.log")
            return
        }

        stage = .finalizing
        do {
            try finalizeMedia()
        } catch {
            stage = .failed("설치 결과 반영 실패: \(error.localizedDescription)")
            return
        }
        stage = .done
        appendLog("Windows 설치가 완료되었습니다.")
        onCompleted?()
    }

    private func failUnlessCancelled(_ message: String) {
        if cancelled {
            cleanupTemporaryMedia()
            stage = .failed("설치가 취소되었습니다.")
        } else {
            stage = .failed(message)
        }
    }

    private func cleanupTemporaryMedia() {
        let fm = FileManager.default
        for path in [plan.tmpTargetPath, plan.tmpVarsPath] {
            try? fm.removeItem(atPath: path)
        }
    }

    private func finalizeMedia() throws {
        let fm = FileManager.default
        for sub in ["disks", "metadata", "logs/hvf"] {
            try fm.createDirectory(
                at: URL(fileURLWithPath: plan.bundlePath).appendingPathComponent(sub),
                withIntermediateDirectories: true)
        }
        try replaceItem(at: plan.bundleDiskPath, withItemAt: plan.tmpTargetPath)
        try replaceItem(at: plan.bundleVarsPath, withItemAt: plan.tmpVarsPath)
        // Seed both the Windows Boot Manager entry and the pinned Microsoft
        // Secure Boot policy. This is deliberately fail-closed: an install is
        // not reported complete with a partially provisioned trust store.
        let secureBootReceipt = try HvfWindowsBootSeed.seedFile(
            varsPath: plan.bundleVarsPath, diskPath: plan.bundleDiskPath)
        let receiptEncoder = JSONEncoder()
        receiptEncoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try receiptEncoder.encode(secureBootReceipt).write(
            to: URL(fileURLWithPath: plan.bundlePath)
                .appendingPathComponent("metadata/secure-boot-provisioning.json"),
            options: [.atomic])
        appendLog("UEFI 부팅 항목과 Microsoft-only Secure Boot 키를 검증·시드했습니다.")
        let installRunLog = URL(fileURLWithPath: plan.tmpEvidenceDir).appendingPathComponent("run.log")
        if fm.fileExists(atPath: installRunLog.path) {
            try? fm.removeItem(atPath: plan.bundleInstallLogPath)
            try? fm.copyItem(atPath: installRunLog.path, toPath: plan.bundleInstallLogPath)
        }
        let controlPath = "\(plan.bundlePath)/metadata/hvf.ctl"
        if !fm.fileExists(atPath: controlPath) {
            fm.createFile(atPath: controlPath, contents: nil)
        }
        // Keep the request as a completed record rather than a pending one.
        let pending = URL(fileURLWithPath: plan.bundlePath)
            .appendingPathComponent(HvfWindowsInstallRequest.fileName)
        let done = URL(fileURLWithPath: plan.bundlePath)
            .appendingPathComponent(HvfWindowsInstallRequest.doneFileName)
        try? fm.removeItem(at: done)
        try? fm.moveItem(at: pending, to: done)
    }

    /// Stage into a bundle-local temporary name first, then rename into place,
    /// so a crash mid-copy never leaves a half-written disk under the final
    /// name. APFS clone keeps the 64 GiB sparse image instant when possible.
    private func replaceItem(at destination: String, withItemAt source: String) throws {
        let fm = FileManager.default
        let staging = destination + ".staging-\(UUID().uuidString)"
        defer { try? fm.removeItem(atPath: staging) }
        do {
            try fm.moveItem(atPath: source, toPath: staging)
        } catch {
            let clone = Shell.run("/bin/cp", ["-c", source, staging])
            if clone.code != 0 {
                try fm.copyItem(atPath: source, toPath: staging)
            }
            try? fm.removeItem(atPath: source)
        }
        if fm.fileExists(atPath: destination) {
            try fm.removeItem(atPath: destination)
        }
        try fm.moveItem(atPath: staging, toPath: destination)
    }

    /// Runs one pipeline Process off the main actor, streaming its stdout and
    /// optionally tailing a separate evidence log for boot progress lines.
    private func runProcess(
        arguments: [String],
        extraEnvironment: [String: String],
        progressLog: URL?
    ) async -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = arguments
        process.currentDirectoryURL = plan.repoRoot
        var environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
        environment["PATH"] = "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin"
        for (key, value) in extraEnvironment { environment[key] = value }
        process.environment = environment
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        currentProcess = process

        appendLog("$ \(arguments.joined(separator: " "))")
        startProgressTimer(progressLog: progressLog)
        defer {
            stopProgressTimer()
            currentProcess = nil
        }

        let accumulator = LineAccumulator()
        pipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            guard !data.isEmpty else { return }
            let lines = accumulator.append(data)
            guard !lines.isEmpty else { return }
            Task { @MainActor [weak self] in
                for line in lines { self?.appendLog(line) }
            }
        }

        return await withCheckedContinuation { continuation in
            process.terminationHandler = { finished in
                pipe.fileHandleForReading.readabilityHandler = nil
                let ok = finished.terminationStatus == 0
                continuation.resume(returning: ok)
            }
            do {
                try process.run()
            } catch {
                pipe.fileHandleForReading.readabilityHandler = nil
                Task { @MainActor [weak self] in
                    self?.appendLog("실행 실패: \(error.localizedDescription)")
                }
                continuation.resume(returning: false)
            }
        }
    }

    private func startProgressTimer(progressLog: URL?) {
        guard let progressLog else { return }
        evidenceTail = TailOffsetReader()
        logTimer = Timer.scheduledTimer(withTimeInterval: 2.0, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                let lines = self.evidenceTail.readNewLines(from: progressLog)
                for line in lines where Self.isProgressLine(line) {
                    self.appendLog(line)
                }
            }
        }
    }

    private func stopProgressTimer() {
        logTimer?.invalidate()
        logTimer = nil
    }

    /// Keep only load-bearing boot lines out of the very chatty run.log.
    nonisolated static func isProgressLine(_ line: String) -> Bool {
        line.contains("BOOT_TIMER ramfb source=") && line.contains("state=captured")
            || line.hasPrefix("BVAGENT ")
            || line.contains("NVMe disk written back")
            || line.contains("stop: PSCI")
    }

    private func appendLog(_ line: String) {
        logLines.append(line)
        if logLines.count > 400 {
            logLines.removeFirst(logLines.count - 400)
        }
    }
}

/// Thread-safe newline splitter for Process pipe callbacks.
final class LineAccumulator: @unchecked Sendable {
    private var buffer = Data()
    private let lock = NSLock()

    /// Split on newlines in one pass, dropping the consumed prefix once.
    ///
    /// Rebuilding the buffer per line copied every remaining byte each time,
    /// so a single large pipe delivery cost time quadratic in its size: 40,000
    /// lines took 497 ms that way against 7 ms here. Installer output arrives
    /// in exactly those bursts.
    func append(_ data: Data) -> [String] {
        lock.lock()
        defer { lock.unlock() }
        buffer.append(data)
        var lines: [String] = []
        var start = buffer.startIndex
        while let newline = buffer[start...].firstIndex(of: 0x0a) {
            let lineData = buffer[start..<newline]
            if let line = String(data: lineData, encoding: .utf8), !line.isEmpty {
                lines.append(line)
            }
            start = buffer.index(after: newline)
        }
        buffer.removeSubrange(..<start)
        return lines
    }
}
