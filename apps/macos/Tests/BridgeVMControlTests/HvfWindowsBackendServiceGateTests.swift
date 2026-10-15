import XCTest
@testable import BridgeVMControl

/// The relay rebuilds its agent harness per boot generation and tails the
/// control file from its current end, so a command appended before
/// `BVAGENT SERVICE start` can be skipped and its reply never arrives.
final class HvfWindowsBackendServiceGateTests: XCTestCase {
    func testGuestCommandBeforeServiceStartIsRefusedWithoutWritingControlFile() throws {
        let fixture = try HvfServiceGateFixture(log: "BVAGENT READY host=test t=1\n")
        defer { fixture.remove() }
        let started = Date()

        let result = fixture.run("whoami", within: 15)

        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.backend.ctlFilePath),
                       "a guest command was appended before the agent service started")
        XCTAssertEqual(result?.output, "HVF 게스트 에이전트 서비스가 아직 시작되지 않아 명령을 보내지 않았습니다. 부팅이 끝난 뒤 다시 시도하세요.")
        XCTAssertEqual(result?.code, -1)
        XCTAssertLessThan(Date().timeIntervalSince(started), 15)
    }

    func testGuestCommandWaitsForServiceStartThenAppendsAndReturnsReply() throws {
        let fixture = try HvfServiceGateFixture(log: "BVAGENT READY host=test t=1\n", agentServiceWait: 10)
        defer { fixture.remove() }

        let pending = fixture.start("whoami")
        usleep(600_000)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.backend.ctlFilePath))
        try fixture.appendLog(HvfAgentServiceStartFixture.marker)
        XCTAssertEqual(fixture.waitForControlFile(), "whoami\n")
        try fixture.appendLog("BVAGENT CMD whoami exit=0\nwin\\dev\nBVAGENT END whoami\n")

        let result = pending.wait(within: 5)
        XCTAssertEqual(result?.output, "win\\dev")
        XCTAssertEqual(result?.code, 0)
    }

    func testGuestCommandReportsStoppedVMWhenItExitsBeforeServiceStart() throws {
        let fixture = try HvfServiceGateFixture(log: nil, agentServiceWait: 60)
        defer { fixture.remove() }
        let started = Date()

        let pending = fixture.start("whoami")
        usleep(300_000)
        fixture.stopVM()
        let result = pending.wait(within: 5)

        XCTAssertLessThan(Date().timeIntervalSince(started), 5)
        XCTAssertEqual(result?.output, "HVF VM이 실행 중이 아닙니다.")
        XCTAssertEqual(result?.code, -1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.backend.ctlFilePath))
    }
}

@MainActor
final class ControlModelServiceGateTests: XCTestCase {
    func testTerminalCommandBeforeServiceStartReportsRefusalAndReleasesBusy() async throws {
        let fixture = try HvfServiceGateFixture(log: nil, agentServiceWait: 0.5)
        defer { fixture.remove() }
        let model = ControlModel(config: fixture.config, backend: fixture.backend, startsAutomatically: false)
        model.running = true
        model.terminalInput = "whoami"

        model.runTerminalCommand()
        let deadline = Date().addingTimeInterval(5)
        while model.busy, Date() < deadline { try await Task.sleep(nanoseconds: 10_000_000) }

        XCTAssertFalse(model.busy)
        XCTAssertTrue(model.terminalLog.contains(HvfAgentServiceGate.serviceNotStartedMessage), model.terminalLog)
        XCTAssertTrue(model.terminalLog.contains("[exit -1]"), model.terminalLog)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.backend.ctlFilePath))
    }
}

/// Seeds the run.log service marker that guest commands wait for.
enum HvfAgentServiceStartFixture {
    static let marker = "BVAGENT SERVICE start t=1\n"

    static func seed(_ backend: HvfWindowsBackend) throws {
        try FileManager.default.createDirectory(atPath: backend.evidenceDir, withIntermediateDirectories: true)
        try marker.write(toFile: backend.evidenceDir + "/run.log", atomically: true, encoding: .utf8)
    }
}

/// A backend whose VM reports running until `stopVM()`/`remove()`, so a
/// command stuck in the 900 s reply wait is released when the test ends.
final class HvfServiceGateFixture {
    let dir: URL
    let config: VMConfig
    let backend: HvfWindowsBackend
    private let running: HvfServiceGateRunningFlag
    private let inFlight = DispatchGroup()

    init(log: String?, agentServiceWait: TimeInterval = HvfAgentServiceGate.guestCommandWait) throws {
        dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString, isDirectory: true)
        config = VMConfig(id: "win-hvf", name: "Win HVF", displayName: "Windows Preview", backendKind: "hvf-engine",
                          bootMode: "windows-hvf", bundlePath: dir.appendingPathComponent("bundle.vmbridge").path,
                          runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "", sshUser: "",
                          leasesPath: "", guestName: "win-hvf", displayWidth: 1280, displayHeight: 800,
                          memMiB: 4096, cpuCount: 1)
        let running = HvfServiceGateRunningFlag()
        self.running = running
        backend = HvfWindowsBackend(config, processIsRunning: { _ in running.value }, agentServiceWait: agentServiceWait)
        try FileManager.default.createDirectory(atPath: backend.evidenceDir, withIntermediateDirectories: true)
        if let log { try appendLog(log) }
    }

    var runLog: URL { URL(fileURLWithPath: backend.evidenceDir).appendingPathComponent("run.log") }

    func appendLog(_ text: String) throws {
        if !FileManager.default.fileExists(atPath: runLog.path) {
            FileManager.default.createFile(atPath: runLog.path, contents: nil)
        }
        let handle = try FileHandle(forWritingTo: runLog)
        defer { try? handle.close() }
        try handle.seekToEnd()
        try handle.write(contentsOf: Data(text.utf8))
    }

    func start(_ command: String) -> HvfServiceGatePendingCommand {
        let pending = HvfServiceGatePendingCommand()
        inFlight.enter()
        DispatchQueue.global().async { [backend, inFlight] in
            pending.finish(backend.runInGuest(command))
            inFlight.leave()
        }
        return pending
    }

    /// Nil when the command is still blocked after `timeout`.
    func run(_ command: String, within timeout: TimeInterval) -> (output: String, code: Int32)? {
        start(command).wait(within: timeout)
    }

    func waitForControlFile(timeout: TimeInterval = 3) -> String? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let text = try? String(contentsOfFile: backend.ctlFilePath, encoding: .utf8), !text.isEmpty { return text }
            usleep(10_000)
        }
        return nil
    }

    func stopVM() { running.value = false }

    func remove() {
        stopVM()
        _ = inFlight.wait(timeout: .now() + 5)
        try? FileManager.default.removeItem(at: dir)
    }
}

final class HvfServiceGatePendingCommand: @unchecked Sendable {
    private let done = DispatchSemaphore(value: 0)
    private var result: (output: String, code: Int32)?

    func finish(_ value: (output: String, code: Int32)) {
        result = value
        done.signal()
    }

    func wait(within timeout: TimeInterval) -> (output: String, code: Int32)? {
        guard done.wait(timeout: .now() + timeout) == .success else { return nil }
        return result
    }
}

final class HvfServiceGateRunningFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var stored = true
    var value: Bool {
        get { lock.lock(); defer { lock.unlock() }; return stored }
        set { lock.lock(); stored = newValue; lock.unlock() }
    }
}
