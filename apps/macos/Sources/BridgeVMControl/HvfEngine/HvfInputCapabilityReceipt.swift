import Foundation

/// Optional keyset markers extend v3; a host ignores a separate marker keyword it predates.
struct HvfInputCapabilityReceipt {
    private enum Phase { case header, marker, end }
    var command: String { "INPUTCAPS \(id)" }
    private let id: String
    private var accepted: [String] = [], phase = Phase.header
    /// Shift+Enter/Space, then the Ctrl/Shift chord set: each optional marker at most once, in this order.
    private var markers: [String] { ["BVINPUT_KEYS \(id) SHIFT_ENTER_SPACE", "BVINPUT_CHORDS \(id) \(HvfControlChordKeys.token)"] }

    init(id: UUID) { self.id = id.uuidString }

    func supports(_ event: HvfOrderedInputQueue.Event) -> Bool {
        guard case let .key(key) = event else { return true }
        if ["shift+enter", "shift+space"].contains(key) { return accepted.contains(markers[0]) }
        return !HvfControlChordKeys.extended.contains(key) || accepted.contains(markers[1])
    }

    mutating func consume(_ line: String) -> HvfInputCapabilitiesRequest.Outcome? {
        let header = "BVAGENT CMD \(command) exit="
        if line.hasPrefix(header) {
            guard phase == .header else { return .failed(.invalidReceipt) }
            guard line == header + "0" else { return .failed(.unsupported) }
            phase = .marker
        } else if line.hasPrefix("BVINPUT_CAPS \(id) ") {
            guard phase == .marker, line == "BVINPUT_CAPS \(id) 3 TEXTINPUT KEYINPUT POINTERINPUT 65536" else {
                return .failed(.invalidReceipt)
            }
            phase = .end
        } else if line.hasPrefix("BVINPUT_KEYS ") || line.hasPrefix("BVINPUT_CHORDS ") {
            guard phase == .end, let index = markers.firstIndex(of: line),
                  !markers[index...].contains(where: { accepted.contains($0) }) else { return .failed(.invalidReceipt) }
            accepted.append(line)
        } else if line == "BVAGENT END \(command)" {
            return phase == .end ? .supported : .failed(.invalidReceipt)
        }
        return nil
    }
}
