import Foundation

enum HvfPerformanceRisk: String, Equatable {
    case balanced
    case aggressive
}

struct HvfEngineConfig: Equatable {
    var targetDiskPath: String
    var uefiVarsPath: String
    var evidenceDir: String
    /// Per-boot diagnostic watchdog. `nil` is the normal app mode: the VM stays
    /// alive until the guest or user requests shutdown.
    var watchdogMs: Int?
    var ramMiB: Int
    var smpCpus: Int
    var clipboardSync: Bool
    var shareHostDir: String?
    var shareGuestDir: String?
    var virtioNet: Bool
    /// Attach the Intel HDA audio device and play the guest's sound through the
    /// Mac speakers via CoreAudio. On by default for parity with a real machine.
    var audioEnabled: Bool = true
    /// Enable the normal Windows display path proven by the live VirGL
    /// evidence run. When disabled, the wrapper retains its legacy 2D device.
    var virtioGpu3d: Bool
    var nvmeBufferedIO: Bool
    var ctlFilePath: String
    /// WinPE driver-injector image booted as NSID 1 for one firstboot cycle;
    /// staged by the install/import flows through the inject-pending marker.
    var placeholderNsid1Path: String?
    /// Enables the wrapper's BOOT_TIMER ramfb lines; the injection flow uses
    /// the `source=virtio-gpu` capture line as its 3D-active confirmation.
    var bootTimerDesktopAgent: Bool = false
    /// Present while a driver injection is pending. The session renames it to
    /// the done marker once the display switches to the 3D scanout.
    var injectPendingMarkerPath: String?
    /// The app deliberately prefers the high-performance renderer lane. It is
    /// a launch-policy switch only: falling back to balanced does not mutate
    /// the guest disk, firmware vars, or driver package.
    var performanceRisk: HvfPerformanceRisk = .aggressive
    /// Product vTPM state stays in the VM bundle but is encrypted by swtpm with
    /// a device-local Keychain key delivered over an inherited file descriptor.
    var vtpmStateDir: String? = nil
    var swtpmBin: String = VTPMStateSecurity.defaultSwtpmCommand()
    var vtpmKeyID: String? = nil

    static func libraryVM(_ config: VMConfig) -> HvfEngineConfig? {
        guard config.engineKind == .hvfEngine else { return nil }
        // A VM whose unattended install has not completed has no bootable disk
        // yet; the detail view routes it to the install panel instead.
        if config.installPending == true { return nil }
        let evidenceDir = config.bundlePath + "/logs/hvf"
        let injection = pendingInjection(bundlePath: config.bundlePath)
        return HvfEngineConfig(
            targetDiskPath: config.diskPath ?? (config.bundlePath + "/disks/hvf-target.raw"),
            uefiVarsPath: config.bundlePath + "/metadata/hvf-vars.fd",
            evidenceDir: evidenceDir,
            watchdogMs: nil,
            ramMiB: config.memMiB ?? 6144,
            smpCpus: config.cpuCount ?? 4,
            clipboardSync: true,
            shareHostDir: nil,
            shareGuestDir: nil,
            virtioNet: config.networkEnabled ?? true,
            virtioGpu3d: true,
            nvmeBufferedIO: true,
            ctlFilePath: config.bundlePath + "/metadata/hvf.ctl",
            placeholderNsid1Path: injection?.injectorPath,
            bootTimerDesktopAgent: injection != nil,
            injectPendingMarkerPath: injection?.markerPath,
            vtpmStateDir: config.bundlePath + "/metadata/vtpm",
            swtpmBin: VTPMStateSecurity.defaultSwtpmCommand(),
            vtpmKeyID: config.slug
        )
    }

    /// Reads the inject-pending marker (first line = injector image path) and
    /// returns it only when both the marker and the injector image exist.
    static func pendingInjection(
        bundlePath: String,
        fileManager: FileManager = .default
    ) -> (markerPath: String, injectorPath: String)? {
        let markerPath = bundlePath + "/" + HvfWindowsInstallPlan.injectPendingMarker
        guard let data = fileManager.contents(atPath: markerPath),
              let firstLine = String(data: data, encoding: .utf8)?
                  .split(separator: "\n", maxSplits: 1, omittingEmptySubsequences: true)
                  .first
        else { return nil }
        let injectorPath = String(firstLine)
        guard fileManager.isReadableFile(atPath: injectorPath) else { return nil }
        return (markerPath, injectorPath)
    }

    func wrapperArguments() -> [String] {
        var args = [
            "scripts/run-hvf-windows-installed-boot.sh",
            "--target", targetDiskPath,
            "--vars", uefiVarsPath,
            "--evidence-dir", evidenceDir
        ]
        if let watchdogMs {
            args.append(contentsOf: ["--watchdog-ms", String(watchdogMs)])
        } else {
            args.append("--no-watchdog")
        }
        args.append(contentsOf: [
            "--ram-mib", String(ramMiB),
            "--smp-cpus", String(smpCpus),
            "--release",
            "--skip-build",
            "--agent-service-control", ctlFilePath,
            "--agent-service-command", "whoami",
            "--display-export-ppm", "\(evidenceDir)/display.ppm",
            "--display-export-ms", "100",
            "--display-export-fb", "\(evidenceDir)/display.fb",
            "--enable-xhci",
            "--input-control", "\(evidenceDir)/input.ctl"
        ])
        if virtioGpu3d {
            args.append(contentsOf: ["--performance-risk", performanceRisk.rawValue])
        }
        if let vtpmStateDir {
            args.append(contentsOf: [
                "--vtpm-state-dir", vtpmStateDir,
                "--swtpm-bin", swtpmBin,
                "--swtpm-key-stdin"
            ])
        }
        if clipboardSync {
            args.append("--agent-clipboard-sync")
        }
        if let host = shareHostDir, let guest = shareGuestDir, !host.isEmpty, !guest.isEmpty {
            args.append(contentsOf: [
                "--agent-share-host", host,
                "--agent-share-guest", guest,
                "--agent-share-ms", "2000",
                "--agent-share-max-kb", "65536"
            ])
        }
        if virtioNet {
            args.append("--virtio-net")
        }
        if audioEnabled {
            args.append("--hda-coreaudio")
        }
        if nvmeBufferedIO {
            args.append("--nvme-buffered-io")
        }
        if virtioGpu3d {
            args.append(contentsOf: [
                "--virtio-gpu-3d",
                "--virtio-gpu-device-id", "1050",
                "--gpu-trace-protocol", "virgl"
            ])
        }
        if let placeholderNsid1Path {
            args.append(contentsOf: ["--placeholder-nsid1", placeholderNsid1Path])
        }
        if bootTimerDesktopAgent {
            args.append("--boot-timer-desktop-agent")
        }
        return args
    }
}

enum HvfWindowsReadinessScope: String, Equatable {
    case launch
    case release
}

struct HvfWindowsReadinessIssue: Identifiable, Equatable {
    let code: String
    let scope: HvfWindowsReadinessScope
    let summary: String

    var id: String { "\(scope.rawValue):\(code)" }
}

struct HvfWindowsReadinessReport: Equatable {
    let issues: [HvfWindowsReadinessIssue]
    let productLimitations: [String]

    var launchReady: Bool { !issues.contains { $0.scope == .launch } }
    var releaseReady: Bool { launchReady && !issues.contains { $0.scope == .release } }
    var launchBlockers: [HvfWindowsReadinessIssue] { issues.filter { $0.scope == .launch } }
    var releaseBlockers: [HvfWindowsReadinessIssue] { issues.filter { $0.scope == .release } }
}

extension HvfEngineConfig {
    /// Fail-closed launch checks plus deliberately separate product-release
    /// blockers. A development VM may launch without claiming that the custom
    /// Windows HVF path is production ready.
    func readiness(
        repoRoot: URL,
        fileManager: FileManager = .default
    ) -> HvfWindowsReadinessReport {
        var issues: [HvfWindowsReadinessIssue] = []
        func launch(_ code: String, _ summary: String) {
            issues.append(.init(code: code, scope: .launch, summary: summary))
        }
        func release(_ code: String, _ summary: String) {
            issues.append(.init(code: code, scope: .release, summary: summary))
        }

        var isDirectory: ObjCBool = false
        if !fileManager.fileExists(atPath: targetDiskPath, isDirectory: &isDirectory)
            || isDirectory.boolValue {
            launch("target-disk-missing", "Windows 대상 RAW 디스크를 찾을 수 없습니다.")
        } else {
            if !fileManager.isReadableFile(atPath: targetDiskPath)
                || !fileManager.isWritableFile(atPath: targetDiskPath) {
                launch("target-disk-access", "Windows 대상 RAW 디스크에 읽기/쓰기 권한이 필요합니다.")
            }
            let diskBytes = (try? fileManager.attributesOfItem(atPath: targetDiskPath)[.size] as? NSNumber)?.uint64Value ?? 0
            if diskBytes == 0 {
                launch("target-disk-empty", "Windows 대상 RAW 디스크가 비어 있습니다.")
            }
        }

        isDirectory = false
        if !fileManager.fileExists(atPath: uefiVarsPath, isDirectory: &isDirectory) || isDirectory.boolValue {
            launch("uefi-vars-missing", "쓰기 가능한 UEFI vars 파일을 찾을 수 없습니다.")
        } else {
            if !fileManager.isReadableFile(atPath: uefiVarsPath)
                || !fileManager.isWritableFile(atPath: uefiVarsPath) {
                launch("uefi-vars-access", "UEFI vars 파일에 읽기/쓰기 권한이 필요합니다.")
            }
            let varsBytes = (try? fileManager.attributesOfItem(atPath: uefiVarsPath)[.size] as? NSNumber)?.uint64Value ?? 0
            if varsBytes != 64 * 1024 * 1024 {
                launch("uefi-vars-size", "UEFI vars 파일은 정확히 64 MiB여야 합니다.")
            }
        }

        let wrapper = repoRoot.appendingPathComponent("scripts/run-hvf-windows-installed-boot.sh")
        if !fileManager.isExecutableFile(atPath: wrapper.path) {
            launch("boot-wrapper-missing", "설치된 Windows HVF 부팅 wrapper를 실행할 수 없습니다.")
        }
        let runner = repoRoot.appendingPathComponent("target/release/examples/hvf_gic_boot_probe")
        if !fileManager.isExecutableFile(atPath: runner.path) {
            launch("signed-runner-missing", "--skip-build 실행에 필요한 release HVF probe가 없습니다. 빌드·entitlement 서명이 필요합니다.")
        }

        if !(1024...65_536).contains(ramMiB) {
            launch("ram-range", "RAM은 1024~65536 MiB 범위여야 합니다.")
        }
        if !(1...123).contains(smpCpus) {
            launch("cpu-range", "vCPU 수는 1~123 범위여야 합니다.")
        }
        if let watchdogMs, watchdogMs <= 0 {
            launch("watchdog-range", "진단 watchdog은 양수여야 합니다.")
        }

        isDirectory = false
        if fileManager.fileExists(atPath: evidenceDir, isDirectory: &isDirectory), !isDirectory.boolValue {
            launch("evidence-not-directory", "증거 경로가 디렉터리가 아닌 파일입니다.")
        }
        if ctlFilePath == targetDiskPath || ctlFilePath == uefiVarsPath {
            launch("control-path-collision", "게스트 제어 파일은 디스크나 UEFI vars와 같은 경로일 수 없습니다.")
        }
        if let vtpmStateDir {
            if vtpmStateDir == targetDiskPath || vtpmStateDir == uefiVarsPath
                || vtpmStateDir == evidenceDir || vtpmStateDir == ctlFilePath {
                launch("vtpm-path-collision", "vTPM 상태 경로는 디스크, UEFI vars, 증거 또는 제어 경로와 분리해야 합니다.")
            }
            if vtpmKeyID?.isEmpty != false {
                launch("vtpm-key-id", "암호화된 vTPM 상태에 사용할 안정적인 VM ID가 없습니다.")
            }
            if !VTPMStateSecurity.executableAvailable(swtpmBin, fileManager: fileManager) {
                launch("swtpm-missing", "vTPM에 필요한 swtpm 실행 파일을 찾을 수 없습니다: \(swtpmBin)")
            }
            isDirectory = false
            if fileManager.fileExists(atPath: vtpmStateDir, isDirectory: &isDirectory),
               !isDirectory.boolValue {
                launch("vtpm-state-not-directory", "vTPM 상태 경로가 디렉터리가 아닌 파일입니다.")
            }
        }
        switch (shareHostDir, shareGuestDir) {
        case (nil, nil): break
        case let (.some(host), .some(guest)):
            isDirectory = false
            if guest.isEmpty || !fileManager.fileExists(atPath: host, isDirectory: &isDirectory)
                || !isDirectory.boolValue {
                launch("share-invalid", "공유 폴더를 쓰려면 존재하는 호스트 디렉터리와 게스트 경로가 모두 필요합니다.")
            }
        default:
            launch("share-incomplete", "호스트/게스트 공유 폴더 경로는 함께 설정해야 합니다.")
        }

        issues.append(contentsOf: Self.permanentReleaseBlockers)
        if virtioGpu3d {
            release("gpu-live-receipt", "최종 서명된 ARM64 viogpu3d의 단일 세대 bind, Status OK, 실타이틀 trace 영수증이 필요합니다.")
        }
        return HvfWindowsReadinessReport(
            issues: issues,
            productLimitations: Self.productLimitations
        )
    }

    /// v1 deliberately exposes stop/start instead of presenting the probe's
    /// single-vCPU checkpoint experiment as a durable suspend contract.
    private static let productLimitations = [
        "v1에서는 Windows HVF suspend/resume을 지원하지 않습니다. 종료 후 다시 시작하세요."
    ]

    private static let permanentReleaseBlockers: [HvfWindowsReadinessIssue] = [
        .init(code: "secure-boot-live-receipt", scope: .release,
              summary: "암호화 vTPM 제품 경로의 Windows TPM/PPI/측정 로그 영수증과 Secure Boot 키 수명주기 검증이 필요합니다."),
        .init(code: "production-driver-signing", scope: .release,
              summary: "테스트 서명이 아닌 배포 가능한 Windows ARM64 드라이버 서명/업데이트 경로가 필요합니다."),
    ]
}
