import Foundation
import BridgeVMWindowProtocol

@main
struct InventoryRestartRegression {
    static func main() throws {
        let now = Date(timeIntervalSince1970: 1000)
        let markers = ["BVAGENT READY host=test", "BVAGENT re-READY t=2",
                       "BVAGENT SERVICE start t=2", "PSCI SYSTEM_RESET: requested", "PSCI_SYSTEM_RESET"]
        var checked = 0
        for marker in markers {
            for position in 0...2 {
                for crlf in [false, true] {
                    var request = HvfWindowInventoryRequest(now: now)
                    var lines = ["BVAGENT \(request.command) WIN 123 7 0 0 640 480 QQ==",
                                 "BVAGENT \(request.command) WINEND"]
                    lines.insert(marker, at: position)
                    if crlf { lines = lines.map { $0 + "\r" } }
                    guard case .failure(.restarted) = request.consume(lines: lines, now: now) else {
                        fatalError("stale inventory published across restart at position \(position)")
                    }
                    precondition(request.consume(lines: [], now: now) == nil)
                    checked += 1
                }
            }
        }
        var normal = HvfWindowInventoryRequest(now: now)
        let rows = ["BVAGENT \(normal.command) WIN 123 7 -10 20 640 480 QQ==",
                    "BVAGENT \(normal.command) WINEND"]
        guard case let .success(records) = normal.consume(lines: rows, now: now),
              records.count == 1, records[0].id == "123" else { fatalError("valid inventory rejected") }
        precondition(normal.consume(lines: [markers[0]], now: now) == nil)
        var pending = HvfWindowInventoryRequest(now: now)
        precondition(pending.consume(lines: ["BVAGENT \(pending.command) WIN 123 7 0 0 1 1 QQ=="], now: now) == nil)
        guard case .failure(.restarted) = pending.consume(
            lines: ["BVAGENT \(pending.command) WINEND", markers[2]], now: now) else {
            fatalError("prior partial inventory escaped restart cancellation")
        }
        print("PASS: \(checked) restart/order/CRLF cases and valid/partial inventory controls; no live Coherence proven")
    }
}
