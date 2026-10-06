import Foundation
import XCTest

/// Disposable host stand-ins only: no product app, guest media or AX permissions.
final class T17OwnedInstallCleanupFixture {
    let root: URL
    let installer = Process()
    var applicationRunning = true
    var presses: [String] = []
    var now = 0.0

    init() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("bridgevm-t17-cleanup-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        installer.executableURL = URL(fileURLWithPath: "/bin/bash")
        installer.environment = ["PATH": "/usr/bin:/bin", "FIXTURE_ROOT": root.path]
        installer.arguments = ["-c", """
        /bin/sleep 5 &
        child=$!
        trap 'kill -TERM "$child" 2>/dev/null; wait "$child"; printf closed > "$FIXTURE_ROOT/closed"; exit 130' TERM INT
        printf ready > "$FIXTURE_ROOT/ready"
        wait "$child"
        printf closed > "$FIXTURE_ROOT/closed"
        """]
        installer.standardOutput = FileHandle.nullDevice
        installer.standardError = FileHandle.nullDevice
        try installer.run()
        let deadline = Date().addingTimeInterval(2)
        while !exists("ready"), installer.isRunning, Date() < deadline { Thread.sleep(forTimeInterval: 0.005) }
        guard exists("ready"), installer.isRunning else { finish(); throw NSError(domain: "owned fixture not ready", code: 1) }
    }

    func press(_ identifier: String, _: TimeInterval) throws {
        presses.append(identifier)
        guard identifier == "bridgevm.windows.install.cancel" else {
            throw NSError(domain: "runtime control absent during installation", code: 1)
        }
        if installer.isRunning { installer.terminate() }
    }

    func stage() -> String? { installer.isRunning ? "취소 중…" : "취소됨" }
    func exists(_ name: String) -> Bool { FileManager.default.fileExists(atPath: root.appendingPathComponent(name).path) }
    func pause(_ seconds: TimeInterval) { now += seconds; Thread.sleep(forTimeInterval: 0.005) }

    func finish() {
        if installer.isRunning { installer.terminate() }
        installer.waitUntilExit() // The owned sleep also expires naturally after five seconds.
        try? FileManager.default.removeItem(at: root)
    }
}
