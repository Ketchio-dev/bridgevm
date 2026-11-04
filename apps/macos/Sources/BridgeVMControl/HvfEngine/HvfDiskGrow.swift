import Foundation

/// A stopped VM's disk grows from its settings (HvfDiskSizeSetting), and C: extends into the new
/// space at the next Windows start.
///
/// The disk is a raw file whose length is the NVMe namespace size, so `snapshot_pair_cli grow`
/// (ftruncate on the selected generation, refused while the VM runs) is instant and sparse: no Mac
/// space is used until Windows writes there. The scripted install leaves ESP, MSR, then C: up to
/// the end of the disk, with no recovery partition after it (scripts/win-assets/bvdiskpart.txt),
/// so C: simply extends. A grow leaves `metadata/disk-grow-pending` in the bundle; at the next
/// `BVAGENT SERVICE start` the session runs `extendCommand` through the agent's plain RUN verb,
/// which every agent has, and removes the file once the guest reports. Without a report (no agent)
/// the file stays, and the setting says to extend C: in Disk Management.
enum HvfDiskGrow {
    static let pendingName = "metadata/disk-grow-pending"
    static let bytesPerGiB: UInt64 = 1 << 30
    /// Free space the largest offered size still leaves on the Mac volume.
    static let safetyMarginBytes: UInt64 = 10 << 30
    /// Exit status of `snapshot_pair_cli grow` while a runtime holds the media.
    static let inUseExitStatus: Int32 = 3
    static let extendTimeout: TimeInterval = 180

    static let sparseHelp = "디스크 파일은 sparse 파일이라 늘려도 Windows가 새 공간에 실제로 쓰기 전까지 Mac 저장 공간을 쓰지 않습니다. 늘린 디스크는 줄일 수 없고, 그 전에 만든 스냅샷을 복원하면 디스크도 그때 크기로 돌아갑니다."
    static let pendingNote = "다음 Windows 시작 때 C: 드라이브를 자동으로 확장합니다. 확장되지 않으면 Windows의 디스크 관리에서 C: 볼륨을 확장하세요."
    static let stopFirst = "VM을 완전히 중지한 뒤 디스크 크기를 변경하세요."

    /// Whole GiB sizes above the current one that leave the margin free; nil when none does.
    static func sizeRange(currentBytes: UInt64, availableBytes: UInt64) -> ClosedRange<Int>? {
        let room = availableBytes > safetyMarginBytes ? availableBytes - safetyMarginBytes : 0
        // ftruncate takes a signed off_t.
        let (sum, overflow) = currentBytes.addingReportingOverflow(room)
        let largestBytes = overflow ? UInt64(Int64.max) : min(sum, UInt64(Int64.max))
        let smallest = currentBytes / bytesPerGiB + 1, largest = largestBytes / bytesPerGiB
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

    static func pendingMarker(config: HvfEngineConfig) -> URL? {
        bundle(config: config)?.appendingPathComponent(pendingName)
    }

    // MARK: guest

    /// One agent line that extends C: to the largest size Windows supports and prints
    /// `BVDISK result=<extended|already|failed> [size=<bytes>] [detail=...]`.
    static let extendCommand: String = {
        let script = #"""
            $ErrorActionPreference = 'Stop'
            try {
              $size = (Get-Partition -DriveLetter C).Size
              $max = (Get-PartitionSupportedSize -DriveLetter C).SizeMax
              if ($max -le $size + 1MB) { 'BVDISK result=already size=' + $size; exit }
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
        case failed(detail: String)

        var notice: String {
            switch self {
            case let .extended(bytes):
                return "C: 드라이브를 늘린 디스크에 맞게 확장했습니다" + (bytes.map { " (\(HvfDiskGrow.gibText($0)))." } ?? ".")
            case .already:
                return "C: 드라이브가 이미 디스크 끝까지 확장되어 있습니다."
            case let .failed(detail):
                return "C: 드라이브를 자동으로 확장하지 못했습니다" + (detail.isEmpty ? "" : "(\(detail))")
                    + ". Windows의 디스크 관리에서 C: 볼륨을 확장하세요."
            }
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
        var pendingMarker: URL {
            disk.deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent(HvfDiskGrow.pendingName)
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
        guard completion.succeeded else { throw failure(completion, action: "디스크 크기를 읽지") }
        return try diskBytes(completion.text)
    }

    /// Grows the selected disk to `bytes`, then leaves the marker the next agent start extends C: for.
    static func grow(_ plan: Plan, to bytes: UInt64) throws -> UInt64 {
        let completion = try HvfHelperProcess.run(
            plan.executable, ["grow", plan.disk.path, plan.vars.path, String(bytes)])
        guard completion.succeeded else { throw failure(completion, action: "디스크를 늘리지") }
        let grown = try diskBytes(completion.text)
        try Data("\(grown)\n".utf8).write(to: plan.pendingMarker, options: [.atomic])
        return grown
    }

    private static func diskBytes(_ output: String) throws -> UInt64 {
        let line = output.split(separator: "\n").first { $0.hasPrefix("disk_bytes ") }
        guard let line, let bytes = UInt64(line.dropFirst("disk_bytes ".count)) else {
            throw error("디스크 크기 도우미가 알 수 없는 결과를 보고했습니다.")
        }
        return bytes
    }

    private static func failure(_ completion: HvfHelperProcess.Completion, action: String) -> NSError {
        if completion.reason == .exit && completion.status == inUseExitStatus { return error(stopFirst) }
        return error("\(action) 못했습니다: \(completion.diagnostic)")
    }

    static func error(_ message: String) -> NSError {
        NSError(domain: "BridgeVM.HvfDiskGrow", code: 1, userInfo: [NSLocalizedDescriptionKey: message])
    }
}

extension HvfEngineSession {
    /// At `BVAGENT SERVICE start` after a grow: extend C: once, and say how it went. A reply
    /// without a report keeps the marker for the next agent start.
    @discardableResult
    func extendGrownDiskIfPending(runner: HvfDisplayDriverInstall.Runner? = nil) -> Task<Void, Never>? {
        guard !diskExtendInFlight, let marker = HvfDiskGrow.pendingMarker(config: config),
              FileManager.default.fileExists(atPath: marker.path) else { return nil }
        diskExtendInFlight = true
        let run = runner ?? { [weak self] command, timeout in
            guard let self else { return ("", -1) }
            return await HvfDisplayDriverInstall.runInGuest(command, session: self, timeout: timeout)
        }
        return Task { @MainActor in
            let reply = await run(HvfDiskGrow.extendCommand, HvfDiskGrow.extendTimeout)
            diskExtendInFlight = false
            guard let outcome = HvfDiskGrow.outcome(reply: reply.output) else { return }
            try? FileManager.default.removeItem(at: marker)
            diskGrowNotice = outcome.notice
        }
    }
}
