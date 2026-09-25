import Darwin
import Foundation

enum T17InstallLogStatus: String {
    case present, unavailable
}

struct T17InstallTimeoutSample {
    let hostTotalTicks: UInt64?
    let hostIdleTicks: UInt64?
    let loadPerCoreX100: Int?
    let logStatus: T17InstallLogStatus
    let logBytes: UInt64?

    static let unavailable = Self(hostTotalTicks: nil, hostIdleTicks: nil,
                                  loadPerCoreX100: nil, logStatus: .unavailable, logBytes: nil)
}

struct T17InstallMarkerCounts {
    let boot: Int, dism: Int, shutdown: Int, watchdog: Int
}

enum T17InstallTimeoutDiagnostic {
    static let timeoutDetail = "product install did not reach a terminal UI stage"
    static let runtimeIdentifiers = ["bridgevm.windows.runtime.view", "bridgevm.dashboard.advanced"]

    static func wait(
        timeout: TimeInterval = 1_800,
        clock: () -> TimeInterval = { ProcessInfo.processInfo.systemUptime },
        pause: (TimeInterval) -> Void = { RunLoop.current.run(until: Date().addingTimeInterval($0)) },
        applicationIsRunning: () -> Bool,
        runtimeView: (String) throws -> Void,
        stage: () throws -> String,
        failure: () throws -> String,
        sample: () -> T17InstallTimeoutSample,
        markers: () -> T17InstallMarkerCounts?
    ) throws {
        let probe = T17InstallTimeoutProbe()
        var terminalFailureObserved = false
        do {
            try T17InstallMonitor.wait(timeout: timeout, clock: clock, pause: pause,
                applicationIsRunning: applicationIsRunning,
                installedRuntimeVisible: {
                    for identifier in runtimeIdentifiers {
                        do { try runtimeView(identifier); return true }
                        catch { probe.recordQueryFailure(error, stage: false) }
                    }
                    return false
                }, stage: {
                    let now = clock()
                    probe.sampleIfDue(at: now, capture: sample)
                    do {
                        let value = try stage()
                        probe.recordStage(value, at: now)
                        if value == "실패" { terminalFailureObserved = true }
                        return value
                    } catch {
                        probe.recordQueryFailure(error, stage: true)
                        return nil
                    }
                }, failure: { try? failure() })
        } catch let blocker as T17Blocker {
            guard !terminalFailureObserved, blocker.code == "installer-failed",
                  blocker.detail == timeoutDetail else { throw blocker }
            let now = clock()
            probe.sampleIfDue(at: now, capture: sample)
            let detail = probe.timeoutDetail(at: now, markers: markers())
            throw T17Blocker(code: blocker.code, detail: detail)
        }
    }
}

private final class T17InstallTimeoutProbe {
    private var phase = "unknown"
    private var phaseAt: TimeInterval?
    private var stageOK = 0, stageMissing = 0, stageErrors = 0
    private var runtimeErrors = 0
    private var lastSampleAt: TimeInterval?
    private var sampleCount = 0
    private var lastSample: T17InstallTimeoutSample = .unavailable
    private var previousTicks: (total: UInt64, idle: UInt64)?
    private var minIdlePct: Int?, maxLoadPerCoreX100: Int?
    private var lastLogBytes: UInt64?, lastLogGrowthAt: TimeInterval?

    func recordStage(_ value: String, at now: TimeInterval) {
        stageOK = min(99_999, stageOK + 1)
        let next = Self.phaseToken(value)
        if next != phase { phase = next; phaseAt = now }
    }

    func recordQueryFailure(_ error: Error, stage: Bool) {
        let missing = (error as? T17Blocker)?.detail
            .hasPrefix("required accessibility identifier was not found:") == true
        if stage {
            if missing { stageMissing = min(99_999, stageMissing + 1) }
            else { stageErrors = min(99_999, stageErrors + 1) }
        } else if !missing {
            runtimeErrors = min(99_999, runtimeErrors + 1)
        }
    }

    func sampleIfDue(at now: TimeInterval, capture: () -> T17InstallTimeoutSample) {
        let due = lastSampleAt.map { now - $0 >= 60 } ?? true
        guard sampleCount < 32, due else { return }
        let current = capture()
        sampleCount += 1
        lastSampleAt = now
        lastSample = current
        if let total = current.hostTotalTicks, let idle = current.hostIdleTicks {
            if let previous = previousTicks, total > previous.total, idle >= previous.idle {
                let percent = Int(min(100, Double(idle - previous.idle) * 100 / Double(total - previous.total)))
                minIdlePct = min(minIdlePct ?? percent, percent)
            }
            previousTicks = (total, idle)
        }
        if let load = current.loadPerCoreX100 {
            maxLoadPerCoreX100 = max(maxLoadPerCoreX100 ?? load, min(9_999, max(0, load)))
        }
        if current.logStatus == .present, let bytes = current.logBytes {
            if lastLogBytes.map({ bytes > $0 }) ?? true { lastLogGrowthAt = now }
            lastLogBytes = bytes
        }
    }

    func timeoutDetail(at now: TimeInterval, markers: T17InstallMarkerCounts?) -> String {
        let phaseAge = age(since: phaseAt, now: now)
        let logAge = age(since: lastLogGrowthAt, now: now)
        let markerText = markers.map {
            "boot=\(max(0, min(9_999, $0.boot))),dism=\(max(0, min(9_999, $0.dism))),shutdown=\(max(0, min(9_999, $0.shutdown))),watchdog=\(max(0, min(9_999, $0.watchdog)))"
        } ?? "markers=unavailable"
        let fields = [
            "diag=v1", "phase=\(phase)", "phase_age_s=\(phaseAge)",
            "ax_stage_ok=\(stageOK)", "ax_stage_missing=\(stageMissing)",
            "ax_stage_err=\(stageErrors)", "ax_runtime_err=\(runtimeErrors)",
            "run_log=\(markers == nil ? "unavailable" : "present")",
            "log_bytes=\(lastSample.logBytes.map { String($0) } ?? "na")",
            "log_growth_age_s=\(logAge)", "sample_age_s=\(age(since: lastSampleAt, now: now))", markerText,
            "host_idle_min_pct=\(minIdlePct.map(String.init) ?? "na")",
            "load_per_core_max_x100=\(maxLoadPerCoreX100.map(String.init) ?? "na")",
            "samples=\(sampleCount)",
        ].joined(separator: ",")
        return String("\(T17InstallTimeoutDiagnostic.timeoutDetail); \(fields)".prefix(512))
    }

    private func age(since start: TimeInterval?, now: TimeInterval) -> String {
        guard let start, now.isFinite, start.isFinite else { return "na" }
        return String(Int(min(1_800, max(0, now - start))))
    }

    private static func phaseToken(_ value: String) -> String {
        switch value {
        case "대기": return "idle"
        case "설치 입력 확인": return "validating"
        case "설치 소스 준비": return "preparing-source"
        case "Windows 무인 설치": return "installing"
        case "VM에 반영": return "finalizing"
        case "기존 설치 결과 복구": return "recovering"
        case "취소 중…": return "cancelling"
        case "취소됨": return "cancelled"
        case "완료": return "done"
        case "실패": return "failed"
        default: return "unknown"
        }
    }
}

final class T17InstallEnvironmentSampler {
    private let evidenceDirectory: String

    init(vmSlug: String) {
        evidenceDirectory = "/tmp/bridgevm-appinstall-\(vmSlug)-evidence"
    }

    init(evidenceDirectory: URL) {
        self.evidenceDirectory = evidenceDirectory.path
    }

    func capture() -> T17InstallTimeoutSample {
        let cpu = Self.cpuTicks()
        let log = inspectLog(readTail: false)
        return T17InstallTimeoutSample(
            hostTotalTicks: cpu?.total, hostIdleTicks: cpu?.idle,
            loadPerCoreX100: Self.loadPerCoreX100(),
            logStatus: log.status, logBytes: log.bytes)
    }

    func markers() -> T17InstallMarkerCounts? { inspectLog(readTail: true).markers }

    private func inspectLog(readTail: Bool) ->
        (status: T17InstallLogStatus, bytes: UInt64?, markers: T17InstallMarkerCounts?) {
        let directory = Darwin.open(evidenceDirectory, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { return (.unavailable, nil, nil) }
        defer { Darwin.close(directory) }
        var owner = stat()
        guard fstat(directory, &owner) == 0, owner.st_mode & mode_t(S_IFMT) == mode_t(S_IFDIR),
              owner.st_uid == geteuid() else { return (.unavailable, nil, nil) }
        let descriptor = Darwin.openat(directory, "run.log", O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { return (.unavailable, nil, nil) }
        defer { Darwin.close(descriptor) }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG),
              info.st_uid == geteuid(), info.st_size >= 0 else { return (.unavailable, nil, nil) }
        let bytes = UInt64(info.st_size)
        guard readTail else { return (.present, bytes, nil) }
        let count = Int(min(bytes, 65_536))
        var tail = [UInt8](repeating: 0, count: count)
        let offset = off_t(bytes - UInt64(count))
        let received = tail.withUnsafeMutableBytes {
            Darwin.pread(descriptor, $0.baseAddress, count, offset)
        }
        guard received == count else { return (.unavailable, nil, nil) }
        let lines = String(decoding: tail, as: UTF8.self).split(whereSeparator: \.isNewline)
        let markers = T17InstallMarkerCounts(
            boot: lines.filter { $0.hasPrefix("BOOT_TIMER ramfb source=") }.count,
            dism: lines.filter { $0 == "BVINSTALL DISM APPLY" }.count,
            shutdown: lines.filter { $0.hasPrefix("stop: PSCI ") && $0.hasSuffix("(system off)") }.count,
            watchdog: lines.filter { $0 == "stop: watchdog (CANCELED)" }.count)
        return (.present, bytes, markers)
    }

    private static func loadPerCoreX100() -> Int? {
        var average = 0.0
        guard getloadavg(&average, 1) == 1, average.isFinite else { return nil }
        let cores = max(1, ProcessInfo.processInfo.activeProcessorCount)
        let scaled = average * 100 / Double(cores)
        guard scaled.isFinite else { return nil }
        return Int(min(9_999, max(0, scaled.rounded())))
    }

    private static func cpuTicks() -> (total: UInt64, idle: UInt64)? {
        var info = host_cpu_load_info()
        var count = mach_msg_type_number_t(MemoryLayout<host_cpu_load_info>.size / MemoryLayout<integer_t>.size)
        let host = mach_host_self()
        defer { mach_port_deallocate(mach_task_self_, host) }
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                host_statistics(host, HOST_CPU_LOAD_INFO, $0, &count)
            }
        }
        guard result == KERN_SUCCESS else { return nil }
        let ticks = info.cpu_ticks
        let idle = UInt64(ticks.2)
        return (UInt64(ticks.0) + UInt64(ticks.1) + idle + UInt64(ticks.3), idle)
    }
}
