import Foundation

enum HvfPerformanceRisk: String, Equatable {
    case balanced
    case aggressive
}

struct HvfEngineConfig: Equatable {
    /// Matches the typed product runtime's launch-manifest CPU contract.
    static let supportedCPURange = 1...64
    /// The largest shared-folder file the runtime copies, either way (`--agent-share-max-kb`):
    /// 16 GiB. Files over 8 MiB move in pieces, so this bounds no memory.
    static let shareMaxKiB = 16 * 1024 * 1024

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
    /// evidence run. When disabled, the display is ramfb (plus `virtioGpu2d`).
    var virtioGpu3d: Bool
    /// Attach the 2D virtio-gpu display device (PCI 1AF4:1050) for a guest with
    /// the signed virtio-win display driver; ramfb stays. `virtioGpu3d` wins.
    var virtioGpu2d: Bool = false
    var nvmeBufferedIO: Bool
    /// The user's opt-in: every guest flush waits for F_FULLFSYNC instead of
    /// the ordered default (F_BARRIERFSYNC, durable within about a second).
    var durableDiskWrites: Bool = false
    /// The user's microphone setting. It reaches the VM only with macOS's permission
    /// (HvfMicrophoneSetting.swift); otherwise the guest has no microphone.
    var microphoneEnabled: Bool = false
    /// The user's TCP port forwards (HvfPortForwards.swift); only with `virtioNet`.
    var portForwards: [HvfPortForward] = []
    /// The user's Windows display resolution ("WxH"); nil lets the VMM keep the
    /// firmware's resolution or add its 1280x800 default.
    var guestResolution: String? = nil
    /// The user's opt-in, as vm.json holds it: nil never set, true on, false off again. On, a HiDPI
    /// screen's 2D virtio display asks for the window's backing pixels at the screen's scale; once set,
    /// every request names the Windows scale (HvfDisplayResizeRequest). The app alone applies it.
    var retinaResolution: Bool? = nil
    /// How the live display maps the Command key (HvfHostKeyChord.swift); host-side only.
    var commandKeyMode: HvfCommandKeyMode = .mac
    var ctlFilePath: String
    /// The app deliberately prefers the high-performance renderer lane. It is
    /// a launch-policy switch only: falling back to balanced does not mutate
    /// the guest disk, firmware vars, or driver package.
    var performanceRisk: HvfPerformanceRisk = .aggressive
    /// Product vTPM state stays in the VM bundle but is encrypted by swtpm with
    /// a device-local Keychain key delivered over an inherited file descriptor.
    var vtpmStateDir: String? = nil
    var swtpmBin: String = VTPMStateSecurity.defaultSwtpmCommand()
    var vtpmKeyID: String? = nil
    var allowsExperimental3D: Bool = false
    var libraryContext: HvfLibraryLaunchContext? = nil

    /// Retina resolution needs the 2D virtio display: the signed display driver with 3D off.
    var offersRetinaResolution: Bool { virtioGpu2d && !virtioGpu3d }
    var requestsRetinaResolution: Bool { retinaResolution == true && offersRetinaResolution }
    /// HvfDisplayResizeRequest's `retina`: nil for a VM that never set it, otherwise whether it draws at Retina.
    var retinaResizeSetting: Bool? { retinaResolution == nil ? nil : requestsRetinaResolution }

    /// Versioned launch manifest consumed by bridgevm-hvf-runtime (`hvf-runner --launch-spec`). Field names and the
    /// version constant are that crate's contract; drift fails its parser.
    /// Encoded alongside wrapperArguments() until the runtime owns the full
    /// lifecycle -- the manifest is the product path, the wrapper the
    /// evidence-harness path (PLAN.md R1).
    func launchManifestJSON() -> String {
        func quoted(_ value: String) -> String {
            // The runtime parser refuses escapes by design; refuse here too
            // rather than emit a manifest it will reject.
            precondition(!value.contains("\\") && !value.contains("\""),
                         "manifest paths may not contain quotes or backslashes")
            return "\"\(value)\""
        }
        return """
        {"version": 1, "disk": \(quoted(targetDiskPath)), \
        "uefi_vars": \(quoted(uefiVarsPath)), \
        "ram_mib": \(ramMiB), "vcpus": \(smpCpus)}
        """
    }

    /// The default vector remains the legacy launch contract; owned mode frames stdin/stdout.
    /// `agentAssetsDir`: the app's guest agent files, installed over an older agent in a library VM.
    func runnerArguments(manifestPath: String, runnerPath: String, firmwareCodePath: String, probePath: String, ownedRuntime: Bool = false,
                         agentAssetsDir: String? = nil,
                         microphone: any HvfMicrophonePermission = HvfSystemMicrophonePermission()) -> [String] {
        var args = [
            runnerPath,
            "--launch-spec", manifestPath,
            "--helper", probePath,
            "--helper-firmware", firmwareCodePath,
            "--helper-agent-control", ctlFilePath,
            "--helper-evidence-dir", evidenceDir
        ]
        if let watchdogMs {
            args.append(contentsOf: ["--watchdog-ms", String(watchdogMs)])
        }
        if virtioGpu3d {
            args.append(contentsOf: [
                "--virtio-gpu-3d",
                "--virtio-gpu-device-id", "1050",
                "--gpu-trace-protocol", "virgl"
            ])
        } else if virtioGpu2d {
            args.append("--virtio-gpu")
        }
        if nvmeBufferedIO { args.append("--nvme-buffered-io") }
        if durableDiskWrites { args.append("--nvme-durable-flush") }
        if let guestResolution { args.append(contentsOf: ["--guest-resolution", guestResolution]) }
        if virtioNet { args.append("--virtio-net") }
        if virtioNet {
            for forward in portForwards { args.append(contentsOf: ["--net-forward", "\(forward.host):\(forward.guest)"]) }
        }
        if audioEnabled { args.append("--helper-hda") }
        if audioEnabled && microphoneEnabled && microphone.status == .authorized { args.append("--helper-hda-microphone") }
        if let libraryContext, let agentAssetsDir {
            args.append(contentsOf: HvfAgentUpdate.runnerArguments(bundlePath: libraryContext.config.bundlePath, assetsDir: agentAssetsDir))
        }
        if clipboardSync { args.append("--agent-clipboard-sync") }
        if let host = shareHostDir, let guest = shareGuestDir, !host.isEmpty, !guest.isEmpty {
            args.append(contentsOf: [
                "--agent-share-host", host,
                "--agent-share-guest", guest,
                "--agent-share-ms", "2000",
                "--agent-share-max-kb", String(HvfEngineConfig.shareMaxKiB)
            ])
        }
        if let vtpmStateDir {
            args.append(contentsOf: [
                "--helper-vtpm-state", vtpmStateDir,
                "--helper-swtpm-bin", swtpmBin
            ])
            if !ownedRuntime { args.append("--helper-vtpm-key-stdin") }
        }
        if ownedRuntime { args.append("--owned-runtime-stdio") }
        if ownedRuntime && libraryContext?.e2eHostDiagnosticStopAdmitted == true { args.append("--helper-host-diagnostic-stop") }
        return args
    }

    func wrapperArguments(microphone: any HvfMicrophonePermission = HvfSystemMicrophonePermission()) -> [String] {
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
            // The display window reads only display.fb; the PPM export was a
            // per-interval checksum and re-encode nobody read. Without 3D a ramfb
            // thread fills display.fb at ~30 fps; 100 ms still paces readback.
            "--display-export-ms", "100",
            "--ramfb-display-export-ms", "33",
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
                "--agent-share-max-kb", String(HvfEngineConfig.shareMaxKiB)
            ])
        }
        if virtioNet {
            args.append("--virtio-net")
        }
        if audioEnabled {
            args.append("--hda-coreaudio")
        }
        if audioEnabled && microphoneEnabled && microphone.status == .authorized {
            args.append("--hda-microphone")
        }
        if nvmeBufferedIO {
            args.append("--nvme-buffered-io")
        }
        if durableDiskWrites {
            args.append("--nvme-durable-flush")
        }
        if let guestResolution {
            args.append(contentsOf: ["--guest-resolution", guestResolution])
        }
        if virtioGpu3d {
            args.append(contentsOf: [
                "--virtio-gpu-3d",
                "--virtio-gpu-device-id", "1050",
                "--gpu-trace-protocol", "virgl"
            ])
        } else if virtioGpu2d {
            args.append("--virtio-gpu")
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
        var issues = libraryContext?.readinessIssues ?? []
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
        if !Self.supportedCPURange.contains(smpCpus) {
            launch("cpu-range", "vCPU 수는 1~64 범위여야 합니다 (현재 \(smpCpus)).")
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
            } else if let rejection = HelperCodeSignature.validate(
                helperPath: swtpmBin,
                appTeam: HelperCodeSignature.readTeamIdentifier(
                    ofBinaryAt: Bundle.main.executableURL?.path ?? ""
                ),
                fileManager: fileManager
            ) {
                // A bundled path is not evidence: an app bundle is a directory
                // the user can write to, so the helper can be replaced after
                // install. The signature is what cannot be.
                launch("swtpm-signature", rejection.message)
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
            // A missing or unreachable folder (an unplugged drive, a deleted folder) starts the VM with
            // the share: the runtime shares the folder once it appears, and the app says so
            // (missingShareFolder). Not a relative one: the runtime resolves it from another directory
            // than this check does, so the folder it would wait for is not this one.
            isDirectory = false
            let exists = fileManager.fileExists(atPath: host, isDirectory: &isDirectory)
            if guest.isEmpty || (exists ? !isDirectory.boolValue : !host.hasPrefix("/")) {
                launch("share-invalid", "공유 폴더 경로가 올바르지 않습니다. 설정에서 공유 폴더를 다시 선택하세요.")
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
