import Foundation
import SwiftUI

/// One-click install of the Microsoft-signed Red Hat virtio-win display driver (viogpudo,
/// Win11 ARM64). Nothing is bundled or pushed: on the user's action the guest downloads the
/// pinned upstream archive itself, refuses any other bytes, adds only the three driver files
/// with pnputil and records one `BVDRIVER result=<installed|staged|failed> detail=<...>` line.
///
/// The guest agent runs one command at a time, and clipboard, shared folders and the display
/// applier wait behind it, so the install never runs inside an agent command: one short command
/// locks `C:\ProgramData\BridgeVM\display-driver` to SYSTEM and Administrators, a second writes
/// the installer there and starts it through Win32_Process.Create (no inherited agent handles),
/// then short polls read its result.
enum HvfDisplayDriverInstall {
    static let archiveURL = "https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/upstream-virtio/attestation-virtio-win-prewhql-0.1-299.zip"
    static let archiveBytes = 94_849_704
    static let archiveSHA256 = "ff393c5338c35d1606a0108d2d818a29be48de373be7aac95b632aa5a46aead0"
    static let archiveFolder = "Win11/ARM64/"
    static let driverFiles = ["viogpudo.inf", "viogpudo.cat", "viogpudo.sys"]
    /// The app waits this long for one install, polling every `pollInterval`. The guest
    /// installer keeps going if the app stops waiting; a later click picks it up.
    static let installTimeout: TimeInterval = 15 * 60
    static let pollInterval: TimeInterval = 3

    enum Status: String { case installed, staged, failed }

    struct Outcome: Equatable {
        let status: Status
        let detail: String

        /// `installed` binds now; `staged` sits in the driver store and binds on the next boot
        /// that has the 1AF4:1050 device. Both leave the driver in the guest.
        var leavesDriverInGuest: Bool { status != .failed }

        var message: String {
            switch status {
            case .installed: return "디스플레이 드라이버를 설치했습니다: \(detail)"
            case .staged: return "디스플레이 드라이버를 Windows 드라이버 저장소에 추가했습니다. virtio-gpu 장치가 연결된 다음 부팅부터 적용됩니다."
            case .failed: return "디스플레이 드라이버를 설치하지 못했습니다: \(detail)"
            }
        }
    }

    /// What one launch or poll reply says.
    enum Progress: Equatable {
        /// `BVDRIVER ready` (the folder is locked; launch next), `BVDRIVER started` or
        /// `BVDRIVER running`: no result yet.
        case running
        /// The installer's result line, or why there is none.
        case finished(Outcome)
    }

    static let stillRunning = Outcome(status: .failed,
        detail: "15분 안에 끝나지 않았습니다. 게스트에서 설치가 계속 진행 중일 수 있습니다. 다시 누르면 진행 중인 설치를 이어서 기다립니다.")

    /// install.ps1, the installer (Windows PowerShell 5.1, ASCII), in the locked
    /// `C:\ProgramData\BridgeVM\display-driver` with `launch.token` (the current launch's random
    /// token), `running.lock` (the installer's PID while it runs), `result-<token>.txt` and the kept
    /// `virtio-win.zip`. Three modes:
    /// - `Launch` (an agent command) starts `Run` through Win32_Process.Create, which gives it no
    ///   agent handles, and returns at once: `BVDRIVER started`, or `BVDRIVER running` when an
    ///   install is already running (a second click joins it). A new launch first writes a new
    ///   token, which Run gets on its command line.
    /// - `Poll` (an agent command) prints this launch's result file, else `BVDRIVER running`, else
    ///   a failed result: the installer is gone without one. Only the result named for the token
    ///   counts, and a lock only while its PID is still Run with that token, so a restart, a killed
    ///   installer or a stale or foreign file never blocks the next install or answers for it.
    /// - `Run` downloads with Windows' own curl.exe. A stalled transfer (the VMM's NAT, real
    ///   networks) is cut after 20 s under 20 kB/s, and every attempt is a new curl run on the
    ///   kept partial file, so `-C -` resumes from its real length (curl's own `--retry` discards
    ///   a failed attempt's bytes); ten attempts in a row that add nothing end it.
    ///   fedorapeople.org answers browser-like agents with an HTML bot check, hence
    ///   `-A BridgeVM`. The length and hash pins refuse such a page or a truncated file before
    ///   anything is extracted; only a pin mismatch or a finished install deletes the archive.
    ///   The one result line appears whole (written aside, then renamed) before the lock goes.
    static let script: String = {
        let files = driverFiles.map { "'\($0)'" }.joined(separator: ", ")
        return #"""
        param([ValidateSet('Launch', 'Poll', 'Run')] [string]$Mode = 'Run', [string]$Token = '')
        $ErrorActionPreference = 'Stop'
        $dir = Join-Path $env:ProgramData 'BridgeVM\display-driver'
        $lock = Join-Path $dir 'running.lock'
        $tokenFile = Join-Path $dir 'launch.token'
        function Format-Result([string]$Result, [string]$Detail) { 'BVDRIVER result=' + $Result + ' detail=' + (($Detail -replace '\s+', ' ').Trim()) }
        if ($Mode -ne 'Run') {
          try {
            if (Test-Path -LiteralPath $tokenFile) { $Token = [IO.File]::ReadAllText($tokenFile) }
            $resultFile = Join-Path $dir ('result-' + $Token + '.txt')
            if ($Mode -eq 'Poll' -and $Token -and (Test-Path -LiteralPath $resultFile)) { [IO.File]::ReadAllText($resultFile); exit }
            $id = 0
            if (Test-Path -LiteralPath $lock) { $id = [IO.File]::ReadAllText($lock).Trim() -as [int] }
            $process = $null
            if ($id -gt 0 -and $Token) { $process = Get-CimInstance -ClassName Win32_Process -Filter ('ProcessId=' + $id) }
            if ($null -ne $process -and $process.CommandLine -like ('*\BridgeVM\display-driver\install.ps1" -Mode Run -Token ' + $Token)) {
              'BVDRIVER running pid=' + $id
            } elseif ($Mode -eq 'Poll') {
              Format-Result 'failed' 'the installer stopped without a result (did Windows restart?); install again'
            } else {
              Remove-Item -Path (Join-Path $dir 'result-*') -Force
              $Token = [guid]::NewGuid().ToString('N')
              [IO.File]::WriteAllText($tokenFile, $Token)
              $line = 'powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $PSCommandPath + '" -Mode Run -Token ' + $Token
              $started = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{ CommandLine = $line }
              if ($started.ReturnValue -ne 0) { throw ('Win32_Process.Create returned ' + $started.ReturnValue) }
              [IO.File]::WriteAllText($lock, [string]$started.ProcessId)
              'BVDRIVER started pid=' + $started.ProcessId
            }
          } catch { Format-Result 'failed' $_.Exception.Message }
          exit
        }
        $zip = Join-Path $dir 'virtio-win.zip'
        $work = Join-Path $dir 'driver'
        $log = Join-Path $dir 'curl.log'
        function Get-ZipLength { if (Test-Path -LiteralPath $zip) { (Get-Item -LiteralPath $zip).Length } else { 0 } }
        try {
          if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
          New-Item -ItemType Directory -Path $work | Out-Null
          $curl = Join-Path $env:SystemRoot 'System32\curl.exe'
          if (-not (Test-Path -LiteralPath $curl)) { throw 'curl.exe missing' }
          $misses = 0
          while ((Get-ZipLength) -lt \#(archiveBytes)) {
            $before = Get-ZipLength
            & $curl -sS -A BridgeVM --fail --location --speed-limit 20000 --speed-time 20 --connect-timeout 30 -C '-' --stderr $log -o $zip '\#(archiveURL)'
            $code = $LASTEXITCODE
            if ($code -eq 0) { break }
            if ((Get-ZipLength) -le $before) { $misses++ }
            if ($misses -ge 10) { throw ('curl.exe exit ' + $code + ': ' + (Get-Content -LiteralPath $log -Tail 1)) }
            Start-Sleep -Seconds 3
          }
          $length = Get-ZipLength
          if ($length -ne \#(archiveBytes)) {
            Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
            throw "download is $length bytes, expected \#(archiveBytes)"
          }
          $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
          if ($hash -ne '\#(archiveSHA256)') {
            Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
            throw "download SHA-256 $hash does not match the pinned \#(archiveSHA256)"
          }
          Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
          $archive = [IO.Compression.ZipFile]::OpenRead($zip)
          try {
            foreach ($name in \#(files)) {
              $entry = $archive.GetEntry('\#(archiveFolder)' + $name)
              if ($null -eq $entry) { throw "\#(archiveFolder)$name is missing from the archive" }
              [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $work $name), $true)
            }
          } finally { $archive.Dispose() }
          $output = & pnputil.exe /add-driver (Join-Path $work 'viogpudo.inf') /install | Out-String
          $code = $LASTEXITCODE
          if (@(0, 259, 1641, 3010) -notcontains $code) { throw ('pnputil exit ' + $code + ': ' + $output) }
          $devices = @(Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue | Where-Object { $_.InstanceId -like 'PCI\VEN_1AF4&DEV_1050*' })
          $bound = @($devices | Where-Object { $_.Service -eq 'VioGpuDod' })
          if ($bound.Count -gt 0 -or $output -match 'installed on device') {
            $result = 'installed'
            $device = @($bound + $devices)[0]
            $detail = $device.FriendlyName + ' status=' + $device.Status
            if ($code -eq 3010) { $detail += '; restart Windows to finish' }
          } elseif ($devices.Count -eq 0) {
            $result = 'staged'
            $detail = 'no 1AF4:1050 device yet; it binds on the next boot with one'
          } else {
            throw ('pnputil exit ' + $code + ' left the 1AF4:1050 device without VioGpuDod')
          }
          Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
        } catch {
          $result = 'failed'
          $detail = $_.Exception.Message
        } finally {
          Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
        }
        $temp = Join-Path $dir ('result-' + $Token + '.tmp')
        [IO.File]::WriteAllText($temp, (Format-Result $result $detail))
        [IO.File]::Move($temp, (Join-Path $dir ('result-' + $Token + '.txt')))
        Remove-Item -LiteralPath $lock -Force -ErrorAction SilentlyContinue
        """#
    }()

    /// The first agent command of every install. The agent runs elevated (bvagent-task.ps1:
    /// `-RunLevel Highest`), ordinary users may create folders in ProgramData and own what they
    /// create, so before anything is written `C:\ProgramData\BridgeVM` and its `display-driver`
    /// folder must be writable only by SYSTEM (S-1-5-18) and Administrators (S-1-5-32-544), owned
    /// by Administrators, with nothing inherited. SIDs, not names: Windows ships localized.
    /// A folder already in exactly that state is reused (it keeps the partial download). Any other
    /// folder is renamed aside whole and a new one is created with the locked descriptor, so no
    /// one else can write to it at any moment; neither step follows a junction, whereas relocking
    /// or deleting in place would act through one that another user swaps in. A reparse point, or
    /// a folder that does not read back locked, is refused. Prints `BVDRIVER ready` or one failed
    /// result line.
    static let prepareCommand = encodedPowerShell(#"""
        $ErrorActionPreference = 'Stop'
        try {
          $root = Join-Path $env:ProgramData 'BridgeVM'
          $sid = [Security.Principal.SecurityIdentifier]
          $locked = New-Object Security.AccessControl.DirectorySecurity
          $locked.SetSecurityDescriptorSddlForm('O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)')
          function Test-ReparsePoint([string]$Path) { [bool]([IO.File]::GetAttributes($Path) -band [IO.FileAttributes]::ReparsePoint) }
          function Test-Locked([string]$Path) {
            $acl = Get-Acl -LiteralPath $Path
            $aces = $acl.GetAccessRules($true, $true, $sid) | ForEach-Object { '{0} {1} {2} {3} {4}' -f $_.IdentityReference, $_.AccessControlType, $_.FileSystemRights, $_.InheritanceFlags, $_.IsInherited }
            $acl.AreAccessRulesProtected -and $acl.GetOwner($sid).Value -eq 'S-1-5-32-544' -and (@($aces | Sort-Object) -join '; ') -eq 'S-1-5-18 Allow FullControl ContainerInherit, ObjectInherit False; S-1-5-32-544 Allow FullControl ContainerInherit, ObjectInherit False'
          }
          foreach ($path in $root, (Join-Path $root 'display-driver')) {
            if (Test-Path -LiteralPath $path) {
              if (Test-ReparsePoint $path) { throw "$path is a reparse point (a junction or link); remove it and install again" }
              if (-not (Test-Locked $path)) { [IO.Directory]::Move($path, $path + '.unlocked-' + [guid]::NewGuid().ToString('N')) }
            }
            [void][IO.Directory]::CreateDirectory($path, $locked)
            if ((Test-ReparsePoint $path) -or -not (Test-Locked $path)) { throw "$path could not be locked to SYSTEM and Administrators" }
          }
          'BVDRIVER ready'
        } catch { 'BVDRIVER result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
        """#)

    /// Writes install.ps1 into the folder `prepareCommand` has just locked (only SYSTEM and
    /// Administrators can change it since) and runs its Launch mode. One agent line under cmd.exe's
    /// 8,191-character limit (see HvfClipboardPasteCommandLimit): cmd's `set` carries the script
    /// as UTF-8 base64, which has no cmd.exe special characters, and `-EncodedCommand`
    /// (UTF-16LE base64) needs no quoting. The locking is its own command because it does not fit
    /// in this line next to the script.
    static let launchCommand = "set BVDRIVER_INSTALLER=\(Data(script.utf8).base64EncodedString())&& "
        + encodedPowerShell(#"""
        $ErrorActionPreference = 'Stop'
        $file = Join-Path $env:ProgramData 'BridgeVM\display-driver\install.ps1'
        [IO.File]::WriteAllBytes($file, [Convert]::FromBase64String($env:BVDRIVER_INSTALLER))
        & $file -Mode Launch
        """#)

    static let pollCommand = encodedPowerShell(#"& (Join-Path $env:ProgramData 'BridgeVM\display-driver\install.ps1') -Mode Poll"#)

    private static func encodedPowerShell(_ script: String) -> String {
        let encoded = Data(script.utf16.flatMap { [UInt8($0 & 0xff), UInt8($0 >> 8)] }).base64EncodedString()
        return "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand \(encoded)"
    }

    static func progress(output: String, code: Int32) -> Progress {
        let lines = output.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
        let prefix = "BVDRIVER "
        guard let line = lines.last(where: { $0.hasPrefix(prefix) }) else {
            let tail = lines.last(where: { !$0.isEmpty }) ?? ""
            return .finished(Outcome(status: .failed,
                detail: "게스트가 BVDRIVER 결과를 보고하지 않았습니다(exit \(code))" + (tail.isEmpty ? "." : ": \(tail)")))
        }
        let rest = line.dropFirst(prefix.count)
        let token = String(rest.prefix { !$0.isWhitespace })
        if token == "ready" || token == "started" || token == "running" { return .running }
        guard token.hasPrefix("result="), let status = Status(rawValue: String(token.dropFirst("result=".count))) else {
            return .finished(Outcome(status: .failed, detail: "알 수 없는 게스트 결과: \(line)"))
        }
        var detail = rest.dropFirst(token.count).trimmingCharacters(in: .whitespaces)
        if detail.hasPrefix("detail=") { detail = String(detail.dropFirst("detail=".count)) }
        return .finished(Outcome(status: status, detail: detail))
    }

    @MainActor static func agentReady(_ session: HvfEngineSession) -> Bool {
        guard session.serviceStarted, case .connected = session.connectionState else { return false }
        return true
    }

    @MainActor static func unavailableReason(_ session: HvfEngineSession) -> String? {
        guard case .connected = session.connectionState else { return "Windows를 시작하고 연결될 때까지 기다리세요." }
        return session.serviceStarted ? nil : "Windows 도우미가 준비될 때까지 기다리세요."
    }

    /// Runs one agent command, waiting at most the given time for its reply.
    typealias Runner = @MainActor (String, TimeInterval) async -> (output: String, code: Int32)

    /// Locks the staging folder, launches the guest installer (or joins the running one), then
    /// polls until it reports, for at most `timeout`; a reply may take whatever is left of it.
    /// `clock` is the session's guestNow(), so a pause does not use up the budget.
    @MainActor static func install(runner: Runner, pollInterval: TimeInterval = pollInterval,
                                   timeout: TimeInterval = installTimeout,
                                   clock: @MainActor () -> Date = Date.init) async -> Outcome {
        let deadline = clock().addingTimeInterval(timeout)
        var reply = await runner(prepareCommand, timeout)
        if progress(output: reply.output, code: reply.code) == .running {
            reply = await runner(launchCommand, deadline.timeIntervalSince(clock()))
        }
        while true {
            if case let .finished(outcome) = progress(output: reply.output, code: reply.code) { return outcome }
            guard deadline.timeIntervalSince(clock()) > pollInterval else { return stillRunning }
            try? await Task.sleep(nanoseconds: UInt64(pollInterval * 1_000_000_000))
            reply = await runner(pollCommand, deadline.timeIntervalSince(clock()))
        }
    }

    /// Runs the install and reports the outcome; `onInstalled` sees only the outcomes that leave
    /// the driver in the guest.
    @MainActor static func start(runner: @escaping Runner, hardwareCursor: Bool = true,
                                 clock: @escaping @MainActor () -> Date = Date.init,
                                 onInstalled: @escaping @MainActor (Outcome) -> Void,
                                 completion: @escaping @MainActor (Outcome) -> Void) {
        Task { @MainActor in
            let outcome = await install(runner: runner, clock: clock)
            // A driver update rewrites the INF's HWCursor=0 (HvfDisplayDriverHardwareCursor.swift),
            // which is what the VM's setting turned off wants.
            if outcome.status == .installed && hardwareCursor { _ = await runner(hardwareCursorCommand, 60) }
            if outcome.leavesDriverInGuest { onInstalled(outcome) }
            completion(outcome)
        }
    }

    /// The agent command path of `HvfWindowsBackend.runInGuest` (shell normalization, one
    /// control-file line, the `BVAGENT CMD`/`END` reply) with the session's liveness: the
    /// backend's `pgrep` on the disk path cannot see the owned runtime, whose helper gets
    /// its disk through the environment.
    @MainActor static func runInGuest(_ command: String, session: HvfEngineSession,
                                      timeout: TimeInterval) async -> (output: String, code: Int32) {
        let normalized: String
        switch HvfGuestCommand.normalizeShell(command) {
        case let .success(value): normalized = value
        case let .failure(error): return (error.message, -1)
        }
        guard agentReady(session) else { return (HvfAgentServiceGate.serviceNotStartedMessage, -1) }
        let log = URL(fileURLWithPath: session.config.evidenceDir).appendingPathComponent("run.log")
        let offset = ((try? FileManager.default.attributesOfItem(atPath: log.path))?[.size] as? NSNumber)?.uint64Value ?? 0
        let reader = HvfCommandReplyReader(command: normalized, offset: offset)
        guard session.sendCtl(normalized) else {
            return ("HVF 제어 채널에 명령을 기록하지 못했습니다: \(session.config.ctlFilePath)", -1)
        }
        let deadline = session.guestNow().addingTimeInterval(timeout)
        while session.guestNow() < deadline {
            if let reply = reader.readReply(from: log) { return reply }
            guard agentReady(session) else { return ("HVF 게스트 연결이 명령 실행 중 종료되었습니다.", -1) }
            try? await Task.sleep(nanoseconds: 250_000_000)
        }
        return ("HVF 게스트 명령 응답 시간 초과", -1)
    }
}

/// The per-VM `virtioDisplayDriverInstalled` flag, which attaches the 2D virtio-gpu from the next
/// start. It is saved like the other per-VM settings, through `saveLibraryPreference`, which
/// refuses while the VM runs (a running VM keeps the launch it was admitted with). The install
/// always runs in a running guest, so the flag waits here, per VM for this app session, and the
/// session saves it when it marks the VM stopped, whether or not the VM's view is open.
@MainActor
enum HvfDisplayDriverFlag {
    private(set) static var pendingVMs: Set<String> = []

    /// For an installed or staged outcome: saves now if the VM is stopped, otherwise once it is.
    @discardableResult
    static func record(_ session: HvfEngineSession,
                       publish: @escaping HvfEngineSession.RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard let slug = session.config.libraryContext?.config.slug else { return nil }
        pendingVMs.insert(slug)
        return saveIfStopped(session, publish: publish)
    }

    /// Nil while the VM runs or once the flag is in vm.json; otherwise why the save failed, which
    /// the session also keeps for the VM's view.
    @discardableResult
    static func saveIfStopped(_ session: HvfEngineSession,
                              publish: @escaping HvfEngineSession.RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard let slug = session.config.libraryContext?.config.slug, pendingVMs.contains(slug),
              session.libraryPreferencesEditable else { return nil }
        let message = session.saveLibraryPreference("디스플레이 드라이버 설정", publish: publish) {
            $0.virtioDisplayDriverInstalled = true
        }
        if message == nil { pendingVMs.remove(slug) }
        if session.displayDriverFlagMessage != message { session.displayDriverFlagMessage = message }
        return message
    }
}

/// The VM view's display driver action, enabled only while the guest agent is ready.
struct HvfDisplayDriverInstallButton: View {
    @ObservedObject var session: HvfEngineSession
    let onInstalled: @MainActor (HvfDisplayDriverInstall.Outcome) -> Void
    @State private var installing = false
    @State private var outcome: HvfDisplayDriverInstall.Outcome?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button(action: install) {
                Label("디스플레이 드라이버 설치 (Red Hat virtio-win, Microsoft 서명)", systemImage: "arrow.down.circle")
            }
            .disabled(installing || !HvfDisplayDriverInstall.agentReady(session))
            .accessibilityIdentifier("bridgevm.runtime.display-driver.install")
            .help("게스트가 Red Hat 공식 배포처에서 virtio-win 압축 파일(약 95 MB)을 직접 내려받아 크기와 SHA-256을 확인한 뒤 viogpudo 드라이버만 설치합니다.")
            if !installing, let reason = HvfDisplayDriverInstall.unavailableReason(session) {
                Text(reason).font(.caption).foregroundColor(.secondary)
            }
            if installing {
                Text("설치 중… 게스트가 드라이버 압축 파일(약 95 MB)을 내려받아 설치합니다. 그동안에도 VM을 계속 사용할 수 있습니다.")
                    .font(.caption).foregroundColor(.secondary)
            } else if let outcome {
                Text(outcome.message)
                    .font(.caption).foregroundColor(outcome.leavesDriverInGuest ? .green : .orange)
                    .textSelection(.enabled)
                if outcome.leavesDriverInGuest && session.persistsLibraryPreferences {
                    Text("다음에 VM을 시작하면 새 디스플레이가 연결됩니다.")
                        .font(.caption).foregroundColor(.secondary)
                }
            }
            if let message = session.displayDriverFlagMessage {
                Text(message).font(.caption).foregroundColor(.orange)
            }
        }
        // The session saves the flag when the VM stops; this retries a save that failed then.
        .onAppear { HvfDisplayDriverFlag.saveIfStopped(session) }
    }

    private func install() {
        installing = true
        outcome = nil
        let session = session
        HvfDisplayDriverInstall.start(runner: { await HvfDisplayDriverInstall.runInGuest($0, session: session, timeout: $1) },
                                      hardwareCursor: session.guestHardwareCursorWanted,
                                      clock: { session.guestNow() }, onInstalled: onInstalled) { result in
            installing = false
            outcome = result
        }
    }
}
