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

    private static func keyEventCount(_ command: String) -> Int? {
        guard !command.isEmpty, command.utf8.count <= 32 else { return nil }
        let parts = command.split(separator: "+", omittingEmptySubsequences: false)
        guard let last = parts.last else { return nil }
        let key = String(last)
        let prefix = parts.dropLast().joined(separator: "+")
        let navigation: Set<String> = ["left", "up", "right", "down", "home", "end", "pageup", "pagedown"]
        let named = navigation.union(["backspace", "tab", "enter", "esc", "escape",
                                      "space", "insert", "delete"])
            .union((1...12).map { "f\($0)" })
        let allowed: Bool
        switch prefix {
        case "": allowed = parts.count == 1 && named.contains(key)
        case "shift": allowed = navigation.contains(key) || key == "tab"
        case "ctrl":
            allowed = navigation.contains(key) || ["a", "c", "v", "x", "y", "z", "backspace", "delete"].contains(key)
        case "ctrl+shift": allowed = navigation.contains(key)
        case "alt": allowed = ["tab", "enter", "f4"].contains(key)
        default: allowed = false
        }
        return allowed ? parts.count * 2 : nil
    }
}
