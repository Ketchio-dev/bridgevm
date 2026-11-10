import Foundation

/// Optional keyset markers extend v3; a host ignores a separate marker keyword it predates.
struct HvfInputCapabilityReceipt {
    private enum Phase { case header, marker, end }
    var command: String { "INPUTCAPS \(id)" }
    private let id: String
    private var accepted: [String] = [], phase = Phase.header
    /// Shift+Enter/Space, the Ctrl/Shift chord set, every generic chord, raw-unit scrolling, modifiers held
    /// for a pointer action and the middle button, in any order after BVINPUT_CAPS.
    private var markers: [String] {
        ["BVINPUT_KEYS \(id) SHIFT_ENTER_SPACE", "BVINPUT_CHORDS \(id) \(HvfControlChordKeys.token)",
         "BVINPUT_CHORDS \(id) \(HvfModifierChord.token)", "BVINPUT_POINTER \(id) SCROLL_UNITS",
         "BVINPUT_POINTER \(id) MODIFIERS", "BVINPUT_POINTER \(id) MIDDLE"]
    }
    /// Any modifiers plus a key and a bare modifier tap (HvfModifierChord); the agent builds every key then.
    var modifierChords: Bool { accepted.contains(markers[2]) }
    /// `scrollunits:<v>,<h>@<x>x<y>` (HvfPointerScrollEncoding); an older agent takes `scroll:` notches only.
    var scrollUnits: Bool { accepted.contains(markers[3]) }
    /// A pointer action's held-modifier field (`press:<x>x<y>:cs`, HvfPointerInputEncoding).
    var pointerModifiers: Bool { accepted.contains(markers[4]) }
    /// `middlepress`, `middlerelease` and `middleclick`; `releaseall` then lifts the middle button too.
    var middleButton: Bool { accepted.contains(markers[5]) }

    init(id: UUID) { self.id = id.uuidString }

    func supports(_ event: HvfOrderedInputQueue.Event) -> Bool {
        if case let .pointer(pointer) = event {
            let fields = pointer.split(separator: ":", omittingEmptySubsequences: false)
            return (scrollUnits || fields[0] != "scrollunits") && (pointerModifiers || fields.count < 3)
                && (middleButton || !fields[0].hasPrefix("middle"))
        }
        guard case let .key(key) = event, !modifierChords else { return true }
        if ["shift+enter", "shift+space"].contains(key) { return accepted.contains(markers[0]) }
        if HvfControlChordKeys.extended.contains(key) { return accepted.contains(markers[1]) }
        return HvfModifierChord.isBase(key)
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
        } else if line.hasPrefix("BVINPUT_KEYS ") || line.hasPrefix("BVINPUT_CHORDS ") || line.hasPrefix("BVINPUT_POINTER ") {
            // A marker only ever grants its own keys: a foreign, early, repeated or misspelled one is ignored
            // instead of failing the whole handshake (which would leave the guest with no typed input at all).
            if phase == .end, markers.contains(line), !accepted.contains(line) { accepted.append(line) }
        } else if line == "BVAGENT END \(command)" {
            return phase == .end ? .supported : .failed(.invalidReceipt)
        }
        return nil
    }
}
