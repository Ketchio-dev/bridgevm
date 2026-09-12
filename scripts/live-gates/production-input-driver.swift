import Foundation
import Darwin

@MainActor
final class ProductionInputDiagnostic {
    let driver = HvfSessionInputDriver()
    let control: FileHandle
    let log: FileHandle
    let binding: [String]
    let events: [HvfOrderedInputQueue.Event]
    var pendingBytes = Data()
    var observed: HvfUnicodeInputRequest?
    var ready = false
    var queued = false
    var sent = 0
    var inserted = 0
    var failure: String?

    init(controlPath: String, logPath: String, x: Int, y: Int) throws {
        guard (0...32767).contains(x), (0...32767).contains(y) else {
            throw NSError(domain: "diagnostic", code: 1)
        }
        let output = Darwin.open(controlPath, O_WRONLY | O_APPEND | O_NOFOLLOW)
        guard output >= 0 else { throw NSError(domain: "diagnostic", code: 2) }
        control = FileHandle(fileDescriptor: output, closeOnDealloc: true)
        let input = Darwin.open(logPath, O_RDONLY | O_NOFOLLOW)
        guard input >= 0 else { throw NSError(domain: "diagnostic", code: 3) }
        log = FileHandle(fileDescriptor: input, closeOnDealloc: true)
        // The owning physical runner has already observed service start.
        try log.seekToEnd()
        binding = [controlPath, logPath]
        events = [.text("BridgeVM"), .key("enter"), .text("\u{D55C}\u{AE00}\u{1F642}"),
                  .pointer("click:\(x)x\(y)")]
    }

    func lines() throws -> [String] {
        if let bytes = try log.read(upToCount: 65_536) { pendingBytes.append(bytes) }
        guard pendingBytes.count <= 1_048_576 else { throw NSError(domain: "diagnostic", code: 4) }
        var result: [String] = []
        while let newline = pendingBytes.firstIndex(of: 10) {
            let bytes = pendingBytes[..<newline]
            let line = String(decoding: bytes, as: UTF8.self)
            result.append(line.hasSuffix("\r") ? String(line.dropLast()) : line)
            pendingBytes.removeSubrange(...newline)
        }
        return result
    }

    func send(_ command: String) -> Bool {
        guard failure == nil else { return false }
        let parts = command.split(separator: " ")
        if parts.first != "INPUTCAPS" {
            guard sent < events.count, observed == nil, parts.count == 3,
                  let id = UUID(uuidString: String(parts[1])),
                  let request = HvfUnicodeInputRequest(event: events[sent], now: Date(), id: id),
                  request.command == command else {
                failure = "unexpected-production-command"
                return false
            }
            observed = request
            sent += 1
        }
        do {
            try control.write(contentsOf: Data((command + "\n").utf8))
            return true
        } catch {
            failure = "transport-write-failed"
            return false
        }
    }

    func pump() {
        guard failure == nil, inserted < events.count else { return }
        do {
            let batch = try lines()
            if var request = observed {
                if let outcome = request.consume(lines: batch, now: Date()) {
                    switch outcome {
                    case .inserted: inserted += 1; observed = nil
                    case .failed: failure = "receipt-rejected"
                    }
                } else { observed = request }
            }
            driver.poll(binding: binding, serviceReady: true, lines: batch, send: send)
            if ready && !queued && failure == nil {
                queued = true
                // Admit the whole burst; only the production driver schedules writes.
                for event in events {
                    if !driver.route(event, binding: binding) { failure = "legacy-fallback"; break }
                }
            }
        } catch { failure = "transport-read-failed" }
    }

    func run() -> Bool {
        driver.onDiagnostic = { [weak self] message in
            if message.hasPrefix("ordered input active:") { self?.ready = true }
            else { self?.failure = "production-driver-refused-or-cancelled" }
        }
        driver.onPoll = { [weak self] in self?.pump() }
        driver.beginOwnedBoot(binding: binding)
        let deadline = Date().addingTimeInterval(35)
        while failure == nil && inserted < events.count && Date() < deadline {
            pump()
            // Normal log polling is 100 ms; the real driver's 10 ms timer also runs.
            RunLoop.main.run(until: Date().addingTimeInterval(0.1))
        }
        driver.onPoll = nil
        driver.cancelTarget()
        if failure == nil && inserted != events.count { failure = "deadline" }
        return failure == nil && sent == events.count && inserted == events.count
    }
}

@main
@MainActor
struct ProductionInputMain {
    static func main() {
        var report: [String: Any] = ["schema": "bridgevm.production-input-driver.v1",
                                    "claim_eligible": false, "production_ui_proven": false,
                                    "guest_application_proven": false, "driver_receipts_observed": false]
        var passed = false
        do {
            let args = CommandLine.arguments
            guard args.count == 5, let x = Int(args[3]), let y = Int(args[4]) else {
                throw NSError(domain: "diagnostic", code: 5)
            }
            let diagnostic = try ProductionInputDiagnostic(controlPath: args[1], logPath: args[2], x: x, y: y)
            passed = diagnostic.run()
            report["driver_receipts_observed"] = passed
            report["sent"] = diagnostic.sent
            report["inserted"] = diagnostic.inserted
            report["failure"] = diagnostic.failure ?? "none"
        } catch { report["failure"] = "invalid-invocation-or-transport" }
        if let bytes = try? JSONSerialization.data(withJSONObject: report, options: [.sortedKeys]),
           let text = String(data: bytes, encoding: .utf8) { print(text) }
        exit(passed ? 0 : 1)
    }
}
