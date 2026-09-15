import AppKit
import ApplicationServices

enum HvfAppUIClient {
    @MainActor
    static func saveBeforeWelcome(_ content: NSView, to output: URL) async throws {
        let pid = getpid()
        let data = try await Task.detached(priority: .userInitiated) {
            try collect(pid: pid)
        }.value
        try data.write(to: output.appendingPathComponent("ui-own-process-ax.json"), options: .atomic)
        try HvfAppUIProbe.save(content, to: output, name: "ui-probe-before-welcome.json")
    }

    private nonisolated static func collect(pid: pid_t) throws -> Data {
        let started = ProcessInfo.processInfo.systemUptime
        let deadline = started + 3
        let trusted = AXIsProcessTrusted()
        var pending = [(AXUIElementCreateApplication(pid), 0)]
        var seen: [AXUIElement] = []
        var rows: [[String: Any]] = []
        while rows.count < 24, !pending.isEmpty {
            let remaining = deadline - ProcessInfo.processInfo.systemUptime
            guard remaining > 0.001 else { break }
            let (element, depth) = pending.removeFirst()
            guard !seen.contains(where: { CFEqual($0, element) }) else { continue }
            seen.append(element)
            var elementPID: pid_t = 0
            let pidError = AXUIElementGetPid(element, &elementPID)
            guard pidError == .success, elementPID == pid else {
                rows.append(["depth": depth, "pid_error": Int(pidError.rawValue), "own_process": false])
                continue
            }
            // Timeouts are per object; never change the process-wide AX setting.
            let timeoutError = AXUIElementSetMessagingTimeout(element, Float(min(0.25, remaining)))
            var children: CFArray?
            let attribute = depth == 0 ? kAXWindowsAttribute : kAXChildrenAttribute
            let readError = timeoutError == .success
                ? AXUIElementCopyAttributeValues(element, attribute as CFString, 0, 24, &children)
                : timeoutError
            let values = children as? [AXUIElement] ?? []
            rows.append(["depth": depth, "pid_error": Int(pidError.rawValue), "own_process": true,
                "timeout_error": Int(timeoutError.rawValue), "read_error": Int(readError.rawValue),
                "returned_count": values.count])
            if readError == .success, depth < 4 {
                let capacity = max(0, 24 - rows.count - pending.count)
                pending.append(contentsOf: values.prefix(capacity).map { ($0, depth + 1) })
            }
        }
        let elapsed = ProcessInfo.processInfo.systemUptime - started
        let record: [String: Any] = ["schema_version": 1, "kind": "own-process-accessibility-read",
            "trusted": trusted, "node_limit": 24, "depth_limit": 4, "time_budget_ms": 3000,
            "messaging_timeout_ms": 250, "elapsed_ms": Int(elapsed * 1000),
            "time_budget_exhausted": elapsed >= 3, "nodes": rows, "node_count": rows.count]
        return try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted, .sortedKeys])
    }
}
