import Foundation

/// Encodes only commands supported by the resident guest input module.
struct HvfGuestInputEncoding {
    let verb: String
    let base64: String
    let insertedEventCount: Int

    init?(_ event: HvfOrderedInputQueue.Event) {
        let value: String
        switch event {
        case let .text(text):
            guard !text.isEmpty, text.utf8.count <= 65_536 else { return nil }
            value = text
            verb = "TEXTINPUT"
            insertedEventCount = text.utf16.count * 2
        case let .key(key):
            guard let count = Self.keyEventCount(key) else { return nil }
            value = key
            verb = "KEYINPUT"
            insertedEventCount = count
        case let .pointer(pointer):
            guard let count = HvfPointerInputEncoding.eventCount(pointer) else { return nil }
            value = pointer
            verb = "POINTERINPUT"
            insertedEventCount = count
        }
        base64 = Data(value.utf8).base64EncodedString()
    }

    /// Any generic chord (HvfModifierChord) except Ctrl+Alt+Delete: SendInput cannot raise the secure
    /// attention sequence, so the agent refuses its keys rather than type them into an application. A held key's
    /// half (`down:`/`up:`) inserts half of its tap's events.
    private static func keyEventCount(_ command: String) -> Int? {
        let action = HvfModifierChord.heldAction(command) ?? command
        guard !isSecureAttention(action), let count = HvfModifierChord.eventCount(action) else { return nil }
        return action == command ? count : count / 2
    }

    /// Ctrl+Alt+Delete with any further modifiers: only the VMM's keyboard raises it (HvfSessionInputRouter).
    static func isSecureAttention(_ key: String) -> Bool {
        let parts = key.split(separator: "+")
        return parts.last == "delete" && parts.contains("ctrl") && parts.contains("alt")
    }
}
