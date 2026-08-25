import Foundation
import Combine

// MARK: - Install request (persisted per VM bundle)

/// A pending Windows HVF install described at VM-creation time and executed
/// later from the VM detail panel. Persisted as metadata/hvf-install.json so
/// an interrupted install can be retried after an app relaunch.
struct HvfWindowsInstallRequest: Codable, Equatable {
    var isoPath: String
    var diskGiB: Int
    var injectViogpu3d: Bool
    var driverPackageDir: String?
    var importedMedia: Bool? = nil

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
    var homeDirectory: String = NSHomeDirectory()

    static let minimumDiskGiB = 64
    static var varsTemplateCandidates: [String] {
        var candidates: [String] = []
        if let override = ProcessInfo.processInfo.environment["BRIDGEVM_UEFI_VARS_TEMPLATE"],
           !override.isEmpty {
            candidates.append((override as NSString).expandingTildeInPath)
        }
        candidates.append(contentsOf: [
            "/opt/homebrew/share/qemu/edk2-arm-vars.fd",
            "/usr/local/share/qemu/edk2-arm-vars.fd",
        ])
        return candidates
    }

    static var wimlibCandidates: [String] {
        ["/opt/homebrew/bin/wimlib-imagex", "/usr/local/bin/wimlib-imagex"]
    }

    static let installResourcePaths = [
        "scripts/build-hvf-windows-scripted-source.sh",
        "scripts/run-hvf-windows-scripted-install.sh",
        "target/release/examples/hvf_gic_boot_probe",
        "scripts/win-assets/winpeshl.ini",
        "scripts/win-assets/bvinstall.cmd",
        "scripts/win-assets/bvdiskpart.txt",
    ]

    /// Cache identity changes on a same-name/same-size file replacement too:
    /// the file number catches replacement while mtime catches in-place edits.
    var sourceCacheKey: String {
        let name = URL(fileURLWithPath: request.isoPath).deletingPathExtension().lastPathComponent
        let attributes = (try? FileManager.default.attributesOfItem(atPath: request.isoPath)) ?? [:]
        return "\(VMConfig.slugify(name))-\((attributes[.size] as? NSNumber)?.uint64Value ?? 0)-\((attributes[.systemFileNumber] as? NSNumber)?.uint64Value ?? 0)-\(((attributes[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0).bitPattern)"
    }

    var sourceImagePath: String { "\(homeDirectory)/BridgeVM/bridgevm-app-src/\(sourceCacheKey).raw" }

    var tmpTargetPath: String { "/tmp/bridgevm-appinstall-\(slug)-target.raw" }
    var tmpVarsPath: String { "/tmp/bridgevm-appinstall-\(slug)-vars.fd" }
    var tmpEvidenceDir: String { "/tmp/bridgevm-appinstall-\(slug)-evidence" }

    var bundleDiskPath: String { "\(bundlePath)/disks/hvf-target.raw" }
    var bundleVarsPath: String { "\(bundlePath)/metadata/hvf-vars.fd" }
    var bundleInstallLogPath: String { "\(bundlePath)/logs/install-run.log" }

    var freshTargetSizeBytes: UInt64 { UInt64(request.diskGiB) * 1024 * 1024 * 1024 }

    var varsTemplatePath: String? {
        Self.varsTemplateCandidates.first { FileManager.default.isReadableFile(atPath: $0) }
    }

    var sourceImageIsCached: Bool {
        FileManager.default.isReadableFile(atPath: sourceImagePath)
    }

    // MARK: commands

    /// Stage a: host-side WinPE scripted-installer source build from the ISO.
    func sourceBuildCommand() -> (environment: [String: String], arguments: [String]) {
        (
            environment: [
                "ISO": request.isoPath,
                "OUT": sourceImagePath,
            ],
            arguments: ["bash", "scripts/build-hvf-windows-scripted-source.sh"]
        )
    }

    /// Stage c: the unattended scripted install boot (WIM apply + bcdboot +
    /// unattended OOBE; reboots into the installed OS before exiting).
    func installCommand() -> [String] {
        var arguments = [
            "bash", "scripts/run-hvf-windows-scripted-install.sh",
            "--source", sourceImagePath,
            "--target", tmpTargetPath,
            "--fresh-target-size", String(freshTargetSizeBytes),
            "--vars", tmpVarsPath,
            "--evidence-dir", tmpEvidenceDir,
            "--release",
            "--skip-build",
            "--watchdog-ms", "1500000",
        ]
        if let template = varsTemplatePath {
            arguments.append(contentsOf: ["--vars-template", template])
        }
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
        if let blocker = injectionValidationError() {
            return blocker
        }
        let fm = FileManager.default
        guard fm.isReadableFile(atPath: request.isoPath) else {
            return "Windows 11 ARM64 ISO 파일을 찾을 수 없습니다."
        }
        let requiredResources = Self.installResourcePaths
            + (request.injectViogpu3d ? Self.injectionResourcePaths : [])
        if let missing = requiredResources.first(where: {
            !fm.isReadableFile(atPath: repoRoot.appendingPathComponent($0).path)
        }) {
            return "앱 설치 리소스가 없습니다: \(missing)"
        }
        guard request.diskGiB >= Self.minimumDiskGiB else {
            return "디스크 크기는 최소 \(Self.minimumDiskGiB) GiB여야 합니다."
        }
        guard Self.wimlibCandidates.contains(where: { fm.isExecutableFile(atPath: $0) }) else {
            return "wimlib-imagex가 필요합니다: brew install wimlib"
        }
        guard importsExistingMedia || varsTemplatePath != nil else {
            return "UEFI vars 템플릿(edk2-arm-vars.fd)을 찾을 수 없습니다: brew install qemu"
        }
        guard importsExistingMedia || Self.whitespaceFree(sourceImagePath) else {
            return "홈 디렉터리 경로에 공백이 있으면 설치 소스를 만들 수 없습니다."
        }
        return nil
    }
}

// MARK: - Install session (stage machine + Process orchestration)

enum HvfWindowsInstallStage: Equatable {
    case idle
    case preparingSource
    case preparingImportedMedia
    case buildingInjector
    case installing
    case injecting
    case finalizing
    case done
    case failed(String)

    var label: String {
        switch self {
        case .idle: return "대기"
        case .preparingSource: return "설치 소스 준비 (WIM 분할)"
        case .preparingImportedMedia: return "가져온 미디어 안전 복제"
        case .buildingInjector: return "검증된 3D 인젝터 준비"
        case .installing: return "Windows 무인 설치"
        case .injecting: return "kernel-policy 드라이버 오프라인 주입"
        case .finalizing: return "VM에 반영"
        case .done: return "완료"
        case .failed: return "실패"
        }
    }
}

@MainActor
final class HvfWindowsInstallSession: ObservableObject {
    @Published var stage: HvfWindowsInstallStage = .idle
    @Published private(set) var logLines: [String] = []
    @Published private(set) var startedAt: Date?

    let plan: HvfWindowsInstallPlan
    /// Called on the main actor after a successful finalize so the library can
    /// clear installPending and refresh.
    var onCompleted: (() -> Void)?

    private var currentProcess: Process?
    private var logTimer: Timer?
    private var evidenceTail = TailOffsetReader()
    var cancelled = false

    init(plan: HvfWindowsInstallPlan) {
        self.plan = plan
    }

    var isRunning: Bool {
        switch stage {
        case .preparingSource, .preparingImportedMedia, .buildingInjector,
             .installing, .injecting, .finalizing: return true
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

    /// Runs one pipeline Process off the main actor, streaming its stdout and
    /// optionally tailing a separate evidence log for boot progress lines.
    func runProcess(
        arguments: [String],
        extraEnvironment: [String: String],
        progressLog: URL?
    ) async -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = arguments
        process.currentDirectoryURL = plan.repoRoot
        var environment = ProcessInfo.processInfo.environment.filter { !$0.key.hasPrefix("BRIDGEVM_") }
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

    func appendLog(_ line: String) {
        logLines.append(line)
        if logLines.count > 400 {
            logLines.removeFirst(logLines.count - 400)
        }
    }
}
