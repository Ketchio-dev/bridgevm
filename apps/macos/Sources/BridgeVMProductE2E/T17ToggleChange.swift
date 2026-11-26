enum T17ToggleChange {
    static func set(_ enabled: Bool, identifier: String, value: () -> Bool?, press: () -> Bool) throws {
        let before = value()
        if before == enabled { return }
        let request = "toggle=\(identifier) requested=\(label(enabled)) before=\(label(before))"
        guard press() else {
            throw T17Blocker(code: "ui-element-missing", detail: "\(request): press failed")
        }
        let after = value()
        guard after == enabled else {
            throw T17Blocker(code: "ui-element-missing", detail: "\(request) after=\(label(after)): value did not change as requested")
        }
    }

    private static func label(_ value: Bool?) -> String {
        guard let value else { return "unavailable" }
        return value ? "on" : "off"
    }
}
