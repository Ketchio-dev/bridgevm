import Darwin
import Foundation

/// "Windows 링크를 Mac에서 열기" (vm.json `openGuestLinksOnMac`, nil = off). On, the app registers a per-user
/// "BridgeVM" browser in the guest at each `BVAGENT SERVICE start` (HKCU only) and sends `LINKWATCH`. Once the
/// user picks BridgeVM in Windows' Default apps, each http(s) link Windows opens runs `handlerScript`, which
/// appends it to `%LOCALAPPDATA%\BridgeVM\open-links.txt`; the agent reports it (bvagent.ps1, link watch), the
/// probe checks it and logs `BVAGENT OPENURL at=<unix_ms> <b64>` (agent_console/guest_link.rs), and the app
/// checks it again and opens it in the Mac's default browser. Turned off (saved false), the app removes the
/// registration once per app session; the guest removes only what is there.
enum HvfGuestLinks {
    static let maxURLBytes = 2048
    static let watchCommand = "LINKWATCH"
    static let linePrefix = "BVAGENT OPENURL "

    /// bv-open-url.ps1. Windows runs it as `powershell.exe ... -File "<it>" "%1"`, so the link is its one
    /// argument and only ever data: never cmd, never -Command, never Invoke-Expression. A loopback link means
    /// this Windows itself and opens in Edge here. scripts/test-bvagent-guest-links.ps1 runs these lines.
    static let handlerScript = #"""
# BridgeVM: opens a Windows link in the Mac browser. Windows runs this as
#   powershell.exe ... -File "<this file>" "%1"
if ($args.Count -ne 1) { exit 2 }
$u = [string]$args[0]
if ($u.Length -gt 2048 -or $u -notmatch '\A(?i:https?)://[^\s\x00-\x1f\x7f"]+\z') { exit 2 }
# A quote in the link ends "%1" early, and Windows then passes something other than the link:
# started for this file, the command line must end with the link as it arrived.
if ([Environment]::OSVersion.Platform -eq 'Win32NT' -and [Environment]::GetCommandLineArgs() -contains $PSCommandPath -and
    -not [Environment]::CommandLine.EndsWith(' "' + $u + '"')) { exit 2 }
$uri = $null
if (-not [Uri]::TryCreate($u, [UriKind]::Absolute, [ref]$uri) -or $uri.Host.Length -eq 0) { exit 2 }
# This machine's own address: Edge here (the default browser may be this script), never the Mac.
if ($uri.IsLoopback -or $uri.Host -like '*.localhost') { Start-Process -FilePath ('microsoft-edge:' + $u); exit 0 }
$dir = Join-Path $env:LOCALAPPDATA 'BridgeVM'
$file = Join-Path $dir 'open-links.txt'
$line = [Text.Encoding]::ASCII.GetBytes([Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($u)) + "`r`n")
[void][IO.Directory]::CreateDirectory($dir)
for ($try = 0; $try -lt 40; $try++) {
    try {
        # Past 64 KB the file starts again; the agent reads from its start when it is shorter.
        $mode = [IO.FileMode]::Append
        if ([IO.File]::Exists($file) -and (New-Object IO.FileInfo $file).Length -gt 65536) { $mode = [IO.FileMode]::Create }
        $stream = New-Object IO.FileStream($file, $mode, ([IO.FileAccess]::Write), ([IO.FileShare]::Read))
        try { $stream.Write($line, 0, $line.Length) } finally { $stream.Dispose() }
        exit 0
    } catch { Start-Sleep -Milliseconds 50 }
}
exit 1
"""#

    /// One agent line printing `BVLINKS result=registered` or `result=failed detail=...`. It writes the handler
    /// (only when it changed, and never through a reparse point: the agent is elevated, the folder is the
    /// user's) and the per-user browser registration Windows' Default apps lists.
    static let registerCommand = command(#"""
$ErrorActionPreference = 'Stop'
try {
  $dir = Join-Path $env:LOCALAPPDATA 'BridgeVM'
  $handler = Join-Path $dir 'bv-open-url.ps1'
  $bytes = [Convert]::FromBase64String('\#(Data(handlerScript.utf8).base64EncodedString())')
  [void][IO.Directory]::CreateDirectory($dir)
  foreach ($path in $dir, $handler) {
    if ((Test-Path -LiteralPath $path) -and ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "reparse point: $path" }
  }
  $same = (Test-Path -LiteralPath $handler) -and ([Convert]::ToBase64String([IO.File]::ReadAllBytes($handler)) -ceq [Convert]::ToBase64String($bytes))
  if (-not $same) { [IO.File]::WriteAllBytes($handler, $bytes) }
  $command = '"' + (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') + '" -NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File "' + $handler + '" "%1"'
  $capabilities = 'Software\Clients\StartMenuInternet\BridgeVM\Capabilities'
  $user = [Microsoft.Win32.Registry]::CurrentUser
  foreach ($entry in @(
      @('Software\Classes\BridgeVM.URL', '', 'BridgeVM URL'),
      @('Software\Classes\BridgeVM.URL\shell\open\command', '', $command),
      @('Software\Clients\StartMenuInternet\BridgeVM', '', 'BridgeVM'),
      @($capabilities, 'ApplicationName', 'BridgeVM'),
      @($capabilities, 'ApplicationDescription', 'Opens Windows links in the Mac browser'),
      @("$capabilities\URLAssociations", 'http', 'BridgeVM.URL'),
      @("$capabilities\URLAssociations", 'https', 'BridgeVM.URL'),
      @('Software\RegisteredApplications', 'BridgeVM', $capabilities))) {
    $key = $user.CreateSubKey($entry[0])
    try { $key.SetValue($entry[1], $entry[2]) } finally { $key.Close() }
  }
  'BVLINKS result=registered'
} catch { 'BVLINKS result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
"""#)

    /// One agent line printing `BVLINKS result=removed`, `result=absent` or `result=failed detail=...`. Only
    /// the registration goes: the handler and its link file stay, inert, so nothing elevated deletes in the
    /// user's folder.
    static let removeCommand = command(#"""
$ErrorActionPreference = 'Stop'
try {
  $user = [Microsoft.Win32.Registry]::CurrentUser
  $removed = $false
  foreach ($path in 'Software\Classes\BridgeVM.URL', 'Software\Clients\StartMenuInternet\BridgeVM') {
    $key = $user.OpenSubKey($path)
    if ($null -ne $key) { $key.Close(); $user.DeleteSubKeyTree($path); $removed = $true }
  }
  $apps = $user.OpenSubKey('Software\RegisteredApplications', $true)
  if ($null -ne $apps) {
    try { if ($null -ne $apps.GetValue('BridgeVM')) { $apps.DeleteValue('BridgeVM'); $removed = $true } } finally { $apps.Close() }
  }
  if ($removed) { 'BVLINKS result=removed' } else { 'BVLINKS result=absent' }
} catch { 'BVLINKS result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
"""#)

    private static func command(_ script: String) -> String {
        let encoded = Data(script.utf16.flatMap { [UInt8($0 & 0xff), UInt8($0 >> 8)] }).base64EncodedString()
        return "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand \(encoded)"
    }

    enum Outcome: Equatable {
        case open(URL, host: String)
        /// Not an http(s) link to a named host without a user name, or the probe refused the guest's bytes.
        case refused(host: String?)
        /// The Mac itself: never opened, as the guest could otherwise reach the Mac's own services.
        case loopback(host: String)
        /// Logged more than 5 s ago or over 1 s ahead: an earlier run.log read again (reattach), not a click.
        case stale
    }

    /// What a probe `BVAGENT OPENURL ...` line asks for; nil for any other line.
    static func outcome(of line: String, now: Date) -> Outcome? {
        let line = line.hasSuffix("\r") ? String(line.dropLast()) : line
        guard line.hasPrefix(linePrefix) else { return nil }
        let fields = line.dropFirst(linePrefix.count).split(separator: " ", omittingEmptySubsequences: false)
        guard fields.count == 2, fields[0].hasPrefix("at="), let ms = Int64(fields[0].dropFirst(3)),
              String(ms) == fields[0].dropFirst(3) else { return .refused(host: nil) }
        let age = now.timeIntervalSince1970 - Double(ms) / 1000
        guard age <= 5, age >= -1 else { return .stale }
        guard let data = Data(base64Encoded: String(fields[1])), data.count <= maxURLBytes,
              let text = String(data: data, encoding: .utf8) else { return .refused(host: nil) }
        return checked(text)
    }

    /// The probe already checked these bytes, but run.log is also the guest's output: check again.
    static func checked(_ text: String) -> Outcome {
        let parts = URLComponents(string: text)
        guard text.utf8.count <= maxURLBytes,
              !text.unicodeScalars.contains(where: { $0.properties.isWhitespace || $0.properties.generalCategory == .control }),
              let parts, let scheme = parts.scheme?.lowercased(), scheme == "http" || scheme == "https",
              let host = parts.host, !host.isEmpty, parts.user == nil, parts.password == nil,
              let url = parts.url else { return .refused(host: parts?.host.flatMap { $0.isEmpty ? nil : $0 }) }
        return isLoopback(host) ? .loopback(host: host) : .open(url, host: host)
    }

    /// localhost and *.localhost, and 127/8, 0/8, ::1, :: and their IPv4-mapped forms in any notation
    /// inet_aton takes (browsers read `http://2130706433/` and `http://127.1/` as 127.0.0.1).
    static func isLoopback(_ host: String) -> Bool {
        var name = host.lowercased()
        if name.hasPrefix("["), name.hasSuffix("]") { name = String(name.dropFirst().dropLast()) }
        while name.hasSuffix(".") { name.removeLast() }
        if name == "localhost" || name.hasSuffix(".localhost") { return true }
        var v4 = in_addr()
        if inet_aton(name, &v4) != 0 { return [0, 127].contains(UInt32(bigEndian: v4.s_addr) >> 24) }
        var v6 = in6_addr()
        guard inet_pton(AF_INET6, name, &v6) == 1 else { return false }
        let bytes = withUnsafeBytes(of: v6) { Array($0) }
        if bytes.dropLast().allSatisfy({ $0 == 0 }) { return bytes[15] <= 1 }
        return bytes[0..<10].allSatisfy { $0 == 0 } && bytes[10] == 0xff && bytes[11] == 0xff && [0, 127].contains(bytes[12])
    }

    /// The event feed shows a link by its host only: a URL can carry a token.
    static func feedEvent(_ event: BvAgentEvent) -> BvAgentEvent {
        guard case let .unknown(line) = event, line.hasPrefix(linePrefix + "at="),
              let b64 = line.split(separator: " ").last, let data = Data(base64Encoded: String(b64)),
              let text = String(data: data, encoding: .utf8) else { return event }
        let at = line.split(separator: " ").dropFirst(2).first ?? ""
        return .unknown("BVAGENT OPENURL \(at) host=\(URLComponents(string: text)?.host ?? "?")")
    }
}

/// Opens a checked link in the Mac's default browser; tests substitute their own.
@MainActor
protocol HvfURLOpener {
    func open(_ url: URL) -> Bool
}

#if canImport(AppKit)
import AppKit

struct HvfWorkspaceURLOpener: HvfURLOpener {
    func open(_ url: URL) -> Bool { NSWorkspace.shared.open(url) }
}
#endif

extension HvfEngineSession {
    /// vm.json `openGuestLinksOnMac`: true on, false turned off again, nil never set (off).
    var guestLinksSetting: Bool? { config.libraryContext?.config.openGuestLinksOnMac }
    var guestLinksWanted: Bool { guestLinksSetting == true }

    /// At `BVAGENT SERVICE start`. On: register the handler (an app update may bring a new one) and start the
    /// agent's link watch, every boot. Turned off: remove the registration once per app session.
    func applyGuestLinksSetting() {
        switch guestLinksSetting {
        case true?:
            guestLinksOffSent = false
            if sendCtl(HvfGuestLinks.registerCommand) { _ = sendCtl(HvfGuestLinks.watchCommand) }
        case false?:
            if !guestLinksOffSent { guestLinksOffSent = sendCtl(HvfGuestLinks.removeCommand) }
        case nil:
            break
        }
    }

    /// A probe `BVAGENT OPENURL` line: opened at most once a second while the setting is on, else ignored.
    func guestLinkReported(_ line: String, now: Date = Date()) {
        guard guestLinksWanted, let outcome = HvfGuestLinks.outcome(of: line, now: now) else { return }
        switch outcome {
        case .stale:
            return
        case let .refused(host):
            guestLinkNotice.show(.refused(host: host))
        case let .loopback(host):
            guestLinkNotice.show(.loopback(host: host))
        case let .open(url, host):
            if let last = lastGuestLinkOpen, now.timeIntervalSince(last) < 1 { return guestLinkNotice.show(.tooFrequent(host: host)) }
            lastGuestLinkOpen = now
            guestLinkNotice.show(guestLinkOpener.open(url) ? nil : .failed(host: host))
        }
    }

    /// What the toggle shows afterwards, vm.json's value, plus any refusal. Off after on is saved as false, so
    /// the next agent start removes the registration; never turned on stays absent.
    func applyGuestLinksToggle(_ enabled: Bool,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (isOn: Bool, message: String?) {
        let message = saveLibraryPreference("링크 설정", publish: publish) {
            $0.openGuestLinksOnMac = enabled ? true : ($0.openGuestLinksOnMac == nil ? nil : false)
        }
        return (guestLinksWanted, message)
    }
}
