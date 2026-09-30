import Foundation

enum T17TextEntry {
    static func commit(_ value: String, set: (String) -> Bool, confirm: () -> Bool, read: () throws -> String?) throws {
        guard set(value) else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element does not accept text") }
        guard confirm() else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element did not commit text") }
        guard try read() == value else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element did not retain exact text") }
    }

    /// For fields whose own action or commit sends them: SwiftUI binds a value set through
    /// accessibility only while the field has focus, and confirming would submit it.
    static func fill(_ value: String, focus: () -> Bool, set: (String) -> Bool, read: () throws -> String?) throws {
        guard focus() else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element did not take focus") }
        guard set(value) else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element does not accept text") }
        guard try read() == value else { throw T17Blocker(code: "ui-element-missing", detail: "identified UI element did not retain exact text") }
    }
}
