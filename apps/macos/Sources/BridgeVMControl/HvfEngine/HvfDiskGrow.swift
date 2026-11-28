import Foundation
import SwiftUI

/// A stopped VM's disk grows from its settings (HvfDiskSizeSetting), and C: extends into the new
/// space at the next Windows start.
///
/// The disk is a raw file whose length is the NVMe namespace size, so `snapshot_pair_cli grow`
/// (ftruncate on the selected generation, refused while the VM runs) is instant and sparse: no Mac
/// space is used until Windows writes there. The scripted install leaves ESP, MSR, then C: up to
/// the end of the disk, with no recovery partition after it (scripts/win-assets/bvdiskpart.txt),
/// so C: simply extends. A grow leaves the permanent flag `metadata/disk-grown` in the bundle, and
/// at every `BVAGENT SERVICE start` of that VM the session runs `extendCommand` through the agent's
/// plain RUN verb, which every agent has. The command is idempotent (`already` unless C: can gain
/// more than 1000 MiB), so a disk whose partition table went back to before an extend, as after
/// restoring a snapshot taken between the grow and the next boot, is extended again. The flag holds
/// the consecutive failed reports: `maximumAttempts` of them stop the check until the next grow, and
/// the notice then says to extend C: in Disk Management; a `blocked` report leaves `blocked`, and any
/// other resets it to 0. The session's `diskGrowNotice` is the latest message, so the guest's report
/// replaces the grow's. A blocked C: and the last failure also show once over the VM display
/// (`diskExtendProblem`), since Settings is the only other place that says so.
enum HvfDiskGrow {
    static let grownName = "metadata/disk-grown"
    static let bytesPerGiB: UInt64 = 1 << 30
    /// Free space the largest offered size still leaves on the drive that holds the VM.
    static let safetyMarginBytes: UInt64 = 10 << 30
    /// Exit status of `snapshot_pair_cli grow` while a runtime holds the media.
    static let inUseExitStatus: Int32 = 3
    static let extendTimeout: TimeInterval = 180
    /// Agent starts that try the extend before a failure leaves it to Disk Management.
    static let maximumAttempts = 3

    static let sparseHelp = "Additional space uses Mac storage as Windows writes to it. You can’t shrink the disk here. Restoring an older snapshot returns the disk to its size at that snapshot."
    static let pendingNote = "BridgeVM will extend the C: drive the next time Windows starts. If needed, open Disk Management in Windows and extend the C: volume."
    static let stopFirst = "Shut down Windows before resizing the disk."

    /// Whole GiB sizes at least 1 GiB above the current one (the extend acts only on more than
    /// 1000 MiB) that leave the margin free; nil when none does.
    static func sizeRange(currentBytes: UInt64, availableBytes: UInt64) -> ClosedRange<Int>? {
        let room = availableBytes > safetyMarginBytes ? availableBytes - safetyMarginBytes : 0
        // ftruncate takes a signed off_t.
        let (sum, overflow) = currentBytes.addingReportingOverflow(room)
        let largestBytes = overflow ? UInt64(Int64.max) : min(sum, UInt64(Int64.max))
        let smallest = (currentBytes / bytesPerGiB) + (currentBytes % bytesPerGiB == 0 ? 1 : 2)
        let largest = largestBytes / bytesPerGiB
        guard smallest <= largest else { return nil }
        return Int(smallest)...Int(largest)
    }

    static func gibText(_ bytes: UInt64) -> String {
        let gib = Double(bytes) / Double(bytesPerGiB)
        return gib == gib.rounded() ? "\(Int(gib)) GiB" : String(format: "%.1f GiB", gib)
    }

    /// The bundle whose managed media this configuration boots, as for a powered-off snapshot.
    static func bundle(config: HvfEngineConfig) -> URL? {
        let disk = URL(fileURLWithPath: config.targetDiskPath).standardizedFileURL
        let bundle = disk.deletingLastPathComponent().deletingLastPathComponent()
        guard !config.targetDiskPath.isEmpty,
              disk.path == bundle.appendingPathComponent("disks/hvf-target.raw").path,
              URL(fileURLWithPath: config.uefiVarsPath).standardizedFileURL.path
                == bundle.appendingPathComponent("metadata/hvf-vars.fd").path else { return nil }
        return bundle
    }

    static func grownFlag(config: HvfEngineConfig) -> URL? {
        bundle(config: config)?.appendingPathComponent(grownName)
    }

    /// What an import of a disk under 64 GiB left before 0.2. Nothing read it, so C: never grew:
    /// it becomes the grown flag.
    static let legacyImportMarkerName = "metadata/hvf-grow-pending"

    static func adoptLegacyImportMarker(config: HvfEngineConfig) {
        guard let bundle = bundle(config: config) else { return }
        let legacy = bundle.appendingPathComponent(legacyImportMarkerName)
        let flag = bundle.appendingPathComponent(grownName)
        let fm = FileManager.default
        guard fm.fileExists(atPath: legacy.path) else { return }
        if !fm.fileExists(atPath: flag.path) {
            guard (try? Data("0\n".utf8).write(to: flag, options: [.atomic])) != nil else { return }
        }
        try? fm.removeItem(at: legacy)
    }

    // MARK: guest

    /// One agent line that extends C: to the largest size Windows supports when that gains more
    /// than 1000 MiB, and prints `BVDISK result=<extended|already|blocked|failed> [size=<bytes>] [detail=...]`.
    /// `blocked`: C: cannot grow but the disk has more than 1 GiB unallocated, which a partition after
    /// C: causes (Windows setup puts Recovery there; BridgeVM's own install does not).
    static let extendCommand: String = {
        let script = #"""
            $ErrorActionPreference = 'Stop'
            try {
              $size = (Get-Partition -DriveLetter C).Size
              $max = (Get-PartitionSupportedSize -DriveLetter C).SizeMax
              if ($max -le $size + 1000MB) {
                if ((Get-Disk -Number (Get-Partition -DriveLetter C).DiskNumber).LargestFreeExtent -gt 1GB) { 'BVDISK result=blocked size=' + $size; exit }
                'BVDISK result=already size=' + $size; exit
              }
              Resize-Partition -DriveLetter C -Size $max
              'BVDISK result=extended size=' + (Get-Partition -DriveLetter C).Size
            } catch { 'BVDISK result=failed detail=' + (($_.Exception.Message -replace '\s+', ' ').Trim()) }
            """#
        let encoded = Data(script.utf16.flatMap { [UInt8($0 & 0xff), UInt8($0 >> 8)] }).base64EncodedString()
        return "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand \(encoded)"
    }()

    enum Outcome: Equatable {
        case extended(bytes: UInt64?)
        case already(bytes: UInt64?)
        case blocked(bytes: UInt64?)
        case failed(detail: String)

        var notice: String {
            switch self {
            case let .extended(bytes):
                return "Extended the C: drive to use the larger disk" + (bytes.map { " (\(HvfDiskGrow.gibText($0)))." } ?? ".")
            case .already:
                return "The C: drive already uses all available disk space."
            case .blocked:
                return "A partition after the C: drive (usually Windows Recovery) keeps BridgeVM from extending it, so the added space stays unallocated. To use it, make a new drive from it in Disk Management."
            case let .failed(detail):
                return Self.failedPrefix(detail) + "Open Disk Management in Windows and extend the C: volume."
            }
        }

        /// A failure the next agent start tries again.
        var retryNotice: String {
            guard case let .failed(detail) = self else { return notice }
            return Self.failedPrefix(detail) + "BridgeVM will try again the next time Windows starts."
        }

        private static func failedPrefix(_ detail: String) -> String {
            "Couldn’t extend the C: drive automatically" + (detail.isEmpty ? "" : " (\(detail))") + ". "
        }
    }

    /// The guest's report in a command reply; nil without one (the agent restarted or did not
    /// answer in time), so the extend waits for the next agent start.
    static func outcome(reply: String) -> Outcome? {
        let prefix = "BVDISK result="
        guard let line = reply.split(whereSeparator: \.isNewline)
            .map({ $0.trimmingCharacters(in: .whitespaces) })
            .last(where: { $0.hasPrefix(prefix) }) else { return nil }
        let rest = line.dropFirst(prefix.count)
        let status = rest.prefix { !$0.isWhitespace }
        let fields = rest.dropFirst(status.count).trimmingCharacters(in: .whitespaces)
        let size = fields.hasPrefix("size=") ? UInt64(fields.dropFirst(5).prefix { $0.isNumber }) : nil
        switch status {
        case "extended": return .extended(bytes: size)
        case "already": return .already(bytes: size)
        case "blocked": return .blocked(bytes: size)
        default:
            let detail = fields.hasPrefix("detail=") ? String(fields.dropFirst("detail=".count)) : fields
            return .failed(detail: String(detail.prefix(200)))
        }
    }

    // MARK: host

    struct Plan: Equatable {
        let executable: URL
        let disk: URL
        let vars: URL
        var grownFlag: URL {
            disk.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent(HvfDiskGrow.grownName)
        }
    }

    /// The managed bundle media and the bundled helper, checked as for a powered-off snapshot.
    static func plan(config: HvfEngineConfig, repoRoot: URL) -> Plan? {
        typealias Check = HvfWindowsSnapshotCommand
        guard bundle(config: config) != nil else { return nil }
        let disk = URL(fileURLWithPath: config.targetDiskPath).standardizedFileURL
        let vars = URL(fileURLWithPath: config.uefiVarsPath).standardizedFileURL
        let executable = repoRoot.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        guard [disk, vars, executable].allSatisfy({ Check.canonical($0) && Check.regularFile($0) }),
              FileManager.default.isExecutableFile(atPath: executable.path) else { return nil }
        return Plan(executable: executable, disk: disk, vars: vars)
    }

    /// The selected disk's length (the generation a restore selected, not the logical original).
    static func currentBytes(_ plan: Plan) throws -> UInt64 {
        let completion = try HvfHelperProcess.run(plan.executable, ["size", plan.disk.path, plan.vars.path])
        guard completion.succeeded else { throw failure(completion, action: "read the disk size") }
        return try diskBytes(completion.text)
    }

    /// Grows the selected disk to `bytes`, then sets the flag that makes every agent start extend
    /// C:, with no failed report yet.
    static func grow(_ plan: Plan, to bytes: UInt64) throws -> UInt64 {
        let completion = try HvfHelperProcess.run(
            plan.executable, ["grow", plan.disk.path, plan.vars.path, String(bytes)])
        guard completion.succeeded else { throw failure(completion, action: "expand the disk") }
        let grown = try diskBytes(completion.text)
        try Data("0\n".utf8).write(to: plan.grownFlag, options: [.atomic])
        return grown
    }

    /// Consecutive failed reports the flag records; anything unreadable counts as none.
    static func failedAttempts(_ flag: URL) -> Int {
        Int(flagText(flag)) ?? 0
    }

    static func flagText(_ flag: URL) -> String {
        ((try? String(contentsOf: flag, encoding: .utf8)) ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func diskBytes(_ output: String) throws -> UInt64 {
        let line = output.split(separator: "\n").first { $0.hasPrefix("disk_bytes ") }
        guard let line, let bytes = UInt64(line.dropFirst("disk_bytes ".count)) else {
            NSLog("Disk size: unrecognized helper output: %@", String(output.prefix(500)))
            throw error("Couldn’t read the disk size. Reinstall BridgeVM, then try again.")
        }
        return bytes
    }

    private static func failure(_ completion: HvfHelperProcess.Completion, action: String) -> NSError {
        if completion.reason == .exit && completion.status == inUseExitStatus { return error(stopFirst) }
        // The helper's own text (Rust errors, an output overflow) goes to the log, not on screen.
        NSLog("Disk helper couldn’t %@ (status %d): %@", action, completion.status, completion.diagnostic)
        return error("Couldn’t \(action). Make sure the drive that holds this VM is connected and has free space, then try again.")
    }

    static func error(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.HvfDiskGrow", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}

extension HvfEngineSession {
    /// Grows the stopped VM's disk under a runtime-mutation reservation. The outcome becomes
    /// `diskGrowNotice`, which the guest's extend report replaces once it arrives.
    @discardableResult
    func growDisk(_ plan: HvfDiskGrow.Plan, toGiB gib: Int) -> Task<Void, Never>? {
        guard let reservation = reserveRuntimeMutation(kind: .diskGrow, configuration: config) else {
            diskGrowNotice = HvfDiskGrow.stopFirst
            return nil
        }
        return Task { @MainActor in
            do {
                guard validateRuntimeMutation(reservation) else { throw HvfDiskGrow.error(HvfDiskGrow.stopFirst) }
                try reservation.checkEffect()
                let grown = try await Task.detached {
                    try HvfDiskGrow.grow(plan, to: UInt64(gib) * HvfDiskGrow.bytesPerGiB)
                }.value
                diskGrowNotice = "Expanded the disk to \(HvfDiskGrow.gibText(grown)). " + HvfDiskGrow.pendingNote
            } catch {
                diskGrowNotice = error.localizedDescription
            }
            finishRuntimeMutation(reservation)
        }
    }

    /// At every `BVAGENT SERVICE start` of a VM whose disk the app grew: extend C: if it can gain
    /// space, and say how it went. A reply without a report changes nothing; a failed report counts
    /// toward `maximumAttempts`, after which the check waits for the next grow; any other resets it.
    @discardableResult
    func extendGrownDisk(runner: HvfDisplayDriverInstall.Runner? = nil) -> Task<Void, Never>? {
        HvfDiskGrow.adoptLegacyImportMarker(config: config)
        guard !diskExtendInFlight, let flag = HvfDiskGrow.grownFlag(config: config),
              FileManager.default.fileExists(atPath: flag.path),
              HvfDiskGrow.failedAttempts(flag) < HvfDiskGrow.maximumAttempts else { return nil }
        diskExtendInFlight = true
        let run = runner ?? { [weak self] command, timeout in
            guard let self else { return ("", -1) }
            return await HvfDisplayDriverInstall.runInGuest(command, session: self, timeout: timeout)
        }
        return Task { @MainActor in
            let reply = await run(HvfDiskGrow.extendCommand, HvfDiskGrow.extendTimeout)
            diskExtendInFlight = false
            guard let outcome = HvfDiskGrow.outcome(reply: reply.output) else { return }
            var failures = 0
            if case .failed = outcome { failures = HvfDiskGrow.failedAttempts(flag) + 1 }
            // A blocked C: counts as no failure (each start checks again); the flag remembers it was shown.
            var blocked = false
            if case .blocked = outcome { blocked = true }
            let shownBlocked = HvfDiskGrow.flagText(flag) == "blocked"
            try? Data((blocked ? "blocked\n" : "\(failures)\n").utf8).write(to: flag, options: [.atomic])
            let retried = failures > 0 && failures < HvfDiskGrow.maximumAttempts
            diskGrowNotice = retried ? outcome.retryNotice : outcome.notice
            // What needs the user shows over the VM display once, not only in Settings.
            if blocked ? !shownBlocked : (failures > 0 && !retried) {
                diskExtendProblem = outcome.notice
            } else if !blocked {
                diskExtendProblem = nil
            }
        }
    }
}

/// A C: extend that needs the user (`diskExtendProblem`), over the VM display.
struct HvfDiskExtendNoticeView: View {
    @ObservedObject var session: HvfEngineSession

    var body: some View {
        if let text = session.diskExtendProblem {
            HvfNoticeStrip(identifier: "bridgevm.storage.extend", title: "disk resize notice",
                           announcement: text, dismiss: { session.diskExtendProblem = nil }) {
                Text(text)
            }
        }
    }
}
