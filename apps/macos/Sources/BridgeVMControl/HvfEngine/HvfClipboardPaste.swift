import Foundation

/// Correlates one clipboard write across log chunks before permitting HID paste.
struct HvfClipboardPaste {
    let command: String
    let marker: String
    private let deadline: Date
    private var began = false
    private var acknowledged = false
    private var finished = false

    init(base64: String, now: Date, id: UUID = UUID()) {
        marker = "BVPASTE_READY \(id.uuidString)"
        command = "powershell -NoProfile -STA -Command \"$ErrorActionPreference='Stop';"
            + "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString("
            + "[Convert]::FromBase64String('\(base64)'))) -ErrorAction Stop;"
            + "Write-Output '\(marker)'\""
        deadline = now.addingTimeInterval(30)
    }

    /// nil: still waiting; true: exact successful frame; false: canceled/failed.
    mutating func consume(lines: [String], now: Date) -> Bool? {
        guard !finished else { return nil }
        guard now < deadline else { return finish(false) }
        if lines.contains(where: { $0.hasPrefix("BVAGENT re-READY ") || $0.hasPrefix("BVAGENT READY ")
            || $0.hasPrefix("PSCI SYSTEM_RESET:") || $0.hasPrefix("BVAGENT SERVICE start") }) {
            return finish(false)
        }
        for raw in lines {
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            let header = "BVAGENT CMD \(command) exit="
            if line.hasPrefix(header) {
                guard !began, line == header + "0" else { return finish(false) }
                began = true
            } else if began && line == marker {
                acknowledged = true
            } else if line == "BVAGENT END \(command)" {
                return finish(began && acknowledged)
            }
        }
        return nil
    }

    private mutating func finish(_ value: Bool) -> Bool {
        finished = true
        return value
    }
}
