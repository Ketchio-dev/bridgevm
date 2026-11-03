import Foundation
import SwiftUI

/// One-click install of the Microsoft-signed Red Hat virtio-win display driver (viogpudo,
/// Win11 ARM64). Nothing is bundled or pushed: on the user's action the guest downloads the
/// pinned upstream archive itself, refuses any other bytes, adds only the three driver files
/// with pnputil and prints one `BVDRIVER result=<installed|staged|failed> detail=<...>` line.
enum HvfDisplayDriverInstall {
    static let archiveURL = "https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/upstream-virtio/attestation-virtio-win-prewhql-0.1-299.zip"
    static let archiveBytes = 94_849_704
    static let archiveSHA256 = "ff393c5338c35d1606a0108d2d818a29be48de373be7aac95b632aa5a46aead0"
    static let archiveFolder = "Win11/ARM64/"
    static let driverFiles = ["viogpudo.inf", "viogpudo.cat", "viogpudo.sys"]
    /// A 95 MB download on a slow guest link plus pnputil; the agent keeps a long command in flight.
    static let replyTimeout: TimeInterval = 30 * 60

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

    /// Windows PowerShell 5.1 script. Windows' own curl.exe downloads because it resumes: a
    /// stalled transfer (seen through the VMM's NAT and on real networks) restarts with `-C -`
    /// where Invoke-WebRequest waited 300 s and failed (live, 2026-11-03). fedorapeople.org
    /// answers browser-like agents with an HTML bot check, hence `-A BridgeVM`; the length and
    /// hash pins refuse such a page or a truncated file. `[NullString]::Value`, not `$null`,
    /// would be needed for a .NET string parameter; none is passed here.
    static let script: String = {
        let files = driverFiles.map { "'\($0)'" }.joined(separator: ", ")
        return #"""
        $ErrorActionPreference = 'Stop'
        $work = Join-Path ([IO.Path]::GetTempPath()) 'bridgevm-viogpudo'
        $zip = Join-Path $work 'virtio-win.zip'
        try {
          if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
          New-Item -ItemType Directory -Path $work | Out-Null
          $curl = Join-Path $env:SystemRoot 'System32\curl.exe'
          if (-not (Test-Path -LiteralPath $curl)) { throw 'curl.exe missing' }
          $log = Join-Path $work 'curl.log'
          & $curl -sS -A BridgeVM --fail --location --connect-timeout 30 --speed-limit 20000 --speed-time 20 --retry 10 --retry-all-errors -C '-' --stderr $log -o $zip '\#(archiveURL)'
          if ($LASTEXITCODE -ne 0) { throw ('curl.exe exit ' + $LASTEXITCODE + ': ' + (Get-Content -LiteralPath $log -Tail 1)) }
          $length = (Get-Item -LiteralPath $zip).Length
          if ($length -ne \#(archiveBytes)) { throw "download is $length bytes, expected \#(archiveBytes)" }
          $hash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
          if ($hash -ne '\#(archiveSHA256)') { throw "download SHA-256 $hash does not match the pinned \#(archiveSHA256)" }
          Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
          $archive = [IO.Compression.ZipFile]::OpenRead($zip)
          try {
            foreach ($name in \#(files)) {
              $entry = $archive.GetEntry('\#(archiveFolder)' + $name)
              if ($null -eq $entry) { throw "\#(archiveFolder)$name is missing from the archive" }
              [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $work $name), $true)
            }
          } finally { $archive.Dispose() }
          Remove-Item -LiteralPath $zip -Force
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
        } catch {
          $result = 'failed'
          $detail = $_.Exception.Message
        } finally {
          Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
        }
        Write-Output ('BVDRIVER result=' + $result + ' detail=' + (($detail -replace '\s+', ' ').Trim()))
        """#
    }()

    /// One agent command line: `-EncodedCommand` (UTF-16LE base64) needs no quoting through
    /// cmd.exe and stays under its 8,191-character limit (see HvfClipboardPasteCommandLimit).
    static let command: String = {
        let encoded = Data(script.utf16.flatMap { [UInt8($0 & 0xff), UInt8($0 >> 8)] }).base64EncodedString()
        return "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand \(encoded)"
    }()

    static func parse(output: String, code: Int32) -> Outcome {
        let lines = output.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }
        let prefix = "BVDRIVER result="
        guard let line = lines.last(where: { $0.hasPrefix(prefix) }) else {
            let tail = lines.last(where: { !$0.isEmpty }) ?? ""
            return Outcome(status: .failed, detail: "게스트가 BVDRIVER 결과를 보고하지 않았습니다(exit \(code))" + (tail.isEmpty ? "." : ": \(tail)"))
        }
        let rest = line.dropFirst(prefix.count)
        let token = String(rest.prefix { !$0.isWhitespace })
        var detail = rest.dropFirst(token.count).trimmingCharacters(in: .whitespaces)
        if detail.hasPrefix("detail=") { detail = String(detail.dropFirst("detail=".count)) }
        guard let status = Status(rawValue: token) else {
            return Outcome(status: .failed, detail: "알 수 없는 게스트 결과: \(line)")
        }
        return Outcome(status: status, detail: detail)
    }

    @MainActor static func agentReady(_ session: HvfEngineSession) -> Bool {
        guard session.serviceStarted, case .connected = session.connectionState else { return false }
        return true
    }

    /// Runs the install through `runner` and reports the outcome; `onInstalled` sees only the
    /// outcomes that leave the driver in the guest.
    @MainActor static func start(runner: @escaping @MainActor (String) async -> (output: String, code: Int32),
                                 onInstalled: @escaping @MainActor (Outcome) -> Void,
                                 completion: @escaping @MainActor (Outcome) -> Void) {
        Task { @MainActor in
            let reply = await runner(command)
            let outcome = parse(output: reply.output, code: reply.code)
            if outcome.leavesDriverInGuest { onInstalled(outcome) }
            completion(outcome)
        }
    }

    /// The agent command path of `HvfWindowsBackend.runInGuest` (shell normalization, one
    /// control-file line, the `BVAGENT CMD`/`END` reply) with the session's liveness: the
    /// backend's `pgrep` on the disk path cannot see the owned runtime, whose helper gets
    /// its disk through the environment.
    @MainActor static func runInGuest(_ command: String, session: HvfEngineSession,
                                      timeout: TimeInterval = replyTimeout) async -> (output: String, code: Int32) {
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
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
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
/// always runs in a running guest, so the flag waits here, per VM for this app session, until
/// the VM has fully stopped.
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

    /// Nil while the VM runs or once the flag is in vm.json; otherwise why the save failed.
    @discardableResult
    static func saveIfStopped(_ session: HvfEngineSession,
                              publish: @escaping HvfEngineSession.RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> String? {
        guard let slug = session.config.libraryContext?.config.slug, pendingVMs.contains(slug),
              session.libraryPreferencesEditable else { return nil }
        let message = session.saveLibraryPreference("디스플레이 드라이버 설정", publish: publish) {
            $0.virtioDisplayDriverInstalled = true
        }
        if message == nil { pendingVMs.remove(slug) }
        return message
    }
}

/// The VM view's display driver action, enabled only while the guest agent is ready.
struct HvfDisplayDriverInstallButton: View {
    @ObservedObject var session: HvfEngineSession
    let onInstalled: @MainActor (HvfDisplayDriverInstall.Outcome) -> Void
    @State private var installing = false
    @State private var outcome: HvfDisplayDriverInstall.Outcome?
    @State private var flagMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button(action: install) {
                Label("디스플레이 드라이버 설치 (Red Hat virtio-win, Microsoft 서명)", systemImage: "arrow.down.circle")
            }
            .disabled(installing || !HvfDisplayDriverInstall.agentReady(session))
            .accessibilityIdentifier("bridgevm.runtime.display-driver.install")
            .help("게스트가 Red Hat 공식 배포처에서 virtio-win 압축 파일(약 95 MB)을 직접 내려받아 크기와 SHA-256을 확인한 뒤 viogpudo 드라이버만 설치합니다.")
            if installing {
                Text("설치 중… 게스트가 드라이버 압축 파일(약 95 MB)을 내려받고 있습니다.")
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
            if let flagMessage {
                Text(flagMessage).font(.caption).foregroundColor(.orange)
            }
        }
        .onAppear { flagMessage = HvfDisplayDriverFlag.saveIfStopped(session) }
        .onChange(of: session.connectionState) { _, state in
            if state == .stopped { flagMessage = HvfDisplayDriverFlag.saveIfStopped(session) }
        }
    }

    private func install() {
        installing = true
        outcome = nil
        let session = session
        HvfDisplayDriverInstall.start(runner: { await HvfDisplayDriverInstall.runInGuest($0, session: session) },
                                      onInstalled: onInstalled) { result in
            installing = false
            outcome = result
        }
    }
}
