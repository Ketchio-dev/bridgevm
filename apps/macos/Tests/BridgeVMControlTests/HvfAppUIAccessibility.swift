import AppKit

@MainActor
enum HvfAppUIAccessibility {
    // Follow only the accessibility children of the owned content view. Never
    // inspect the application's other windows, a system AX tree or another app.
    static func elements(in root: NSView) throws -> [any NSAccessibilityProtocol] {
        var pending: [Any] = [root]
        var visited = Set<ObjectIdentifier>()
        var result: [any NSAccessibilityProtocol] = []
        while let next = pending.popLast() {
            guard let object = next as? NSObject else { continue }
            guard visited.insert(ObjectIdentifier(object)).inserted else { continue }
            guard visited.count <= 10_000 else { throw HvfAppUIError.refused("Owned AX tree exceeded its bound") }
            guard let element = object as? any NSAccessibilityProtocol else { continue }
            result.append(element)
            pending.append(contentsOf: element.accessibilityChildren() ?? [])
        }
        return result
    }

    static func find(_ identifier: String, in root: NSView) throws -> (any NSAccessibilityProtocol)? {
        try elements(in: root).first { $0.accessibilityIdentifier() == identifier }
    }

    static func press(_ identifier: String, in root: NSView) throws {
        guard let element = try find(identifier, in: root), element.isAccessibilityEnabled(),
              element.accessibilityPerformPress()
        else { throw HvfAppUIError.refused("Actual AX press unavailable: \(identifier)") }
    }

    static func pressButton(label: String, in root: NSView) throws {
        let candidates = try elements(in: root).filter {
            $0.accessibilityRole() == .button
                && ($0.accessibilityLabel() == label || $0.accessibilityTitle() == label)
        }
        guard candidates.count == 1, let button = candidates.first, button.isAccessibilityEnabled(),
              button.accessibilityPerformPress()
        else { throw HvfAppUIError.refused("Actual unique AX button press unavailable: \(label)") }
    }

    static func setText(_ text: String, identifier: String, in root: NSView) throws {
        guard let element = try find(identifier, in: root), element.accessibilityRole() == .textField,
              element.isAccessibilityEnabled()
        else { throw HvfAppUIError.refused("Actual AX text field unavailable: \(identifier)") }
        element.setAccessibilityValue(text)
        guard element.accessibilityValue() as? String == text
        else { throw HvfAppUIError.refused("AX field rejected its requested value") }
    }

    static func hasCard(named name: String, in root: NSView) throws -> Bool {
        try elements(in: root).contains {
            $0.accessibilityRole() == .button
                && ([$0.accessibilityLabel(), $0.accessibilityTitle()].compactMap { $0 }
                    .contains { $0.hasPrefix(name + ", HVF,") })
        }
    }

    static func wait(_ reason: String, until condition: () throws -> Bool) async throws {
        let deadline = Date().addingTimeInterval(5)
        repeat {
            if try condition() { return }
            try await Task.sleep(nanoseconds: 50_000_000)
        } while Date() < deadline
        throw HvfAppUIError.refused("Timed out waiting for actual UI: \(reason)")
    }
}
