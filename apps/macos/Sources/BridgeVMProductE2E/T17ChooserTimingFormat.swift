import Foundation

enum T17ChooserTimingFormat {
    static func milliseconds(_ time: TimeInterval?) -> String {
        guard let time else { return "unknown" }
        let value = String(format: "%.3f", time * 1000)
        return value.count <= 24 ? value : "out_of_range"
    }

    static func snapshot(header: String, records: [String]) -> String {
        var recent = Array(records.suffix(6))
        while (header + recent.joined(separator: "|")).count > 882 { recent.removeFirst() }
        return header + recent.joined(separator: "|")
    }
}
