import Foundation

/// The optional keyset extends v3; older hosts safely ignore its separate marker.
struct HvfInputCapabilityReceipt {
    private enum Phase { case header, marker, end, keys }
    let command: String
    private let markerPrefix: String
    private let keyMarker: String
    private var phase = Phase.header

    init(id: UUID) {
        command = "INPUTCAPS \(id.uuidString)"
        markerPrefix = "BVINPUT_CAPS \(id.uuidString) "
        keyMarker = "BVINPUT_KEYS \(id.uuidString) SHIFT_ENTER_SPACE"
    }

    func supports(_ event: HvfOrderedInputQueue.Event) -> Bool {
        guard case let .key(key) = event, ["shift+enter", "shift+space"].contains(key) else { return true }
        return phase == .keys
    }

    mutating func consume(_ line: String) -> HvfInputCapabilitiesRequest.Outcome? {
        let header = "BVAGENT CMD \(command) exit="
        if line.hasPrefix(header) {
            guard phase == .header else { return .failed(.invalidReceipt) }
            guard line == header + "0" else { return .failed(.unsupported) }
            phase = .marker
        } else if line.hasPrefix(markerPrefix) {
            guard phase == .marker, line == markerPrefix + "3 TEXTINPUT KEYINPUT POINTERINPUT 65536" else {
                return .failed(.invalidReceipt)
            }
            phase = .end
        } else if line.hasPrefix("BVINPUT_KEYS ") {
            guard phase == .end, line == keyMarker else { return .failed(.invalidReceipt) }
            phase = .keys
        } else if line == "BVAGENT END \(command)" {
            return phase == .end || phase == .keys ? .supported : .failed(.invalidReceipt)
        }
        return nil
    }
}
