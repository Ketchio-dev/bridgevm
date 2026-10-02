import Foundation

/// Names why a display press did not take the ordered path, in the characters the T17 display
/// diagnostic keeps (letters, digits, '-', '=', space). T17 r72 on the Studio pressed the display
/// once and it went "legacy" with no INPUTCAPS exchange in the run log; this records whether the
/// router was owned, how far negotiation got and whether the session was an attachment.
enum HvfInputRouteDiagnostic {
    static func label(owned: Bool, state: HvfNegotiatedInputStream.State, activated: Bool, failed: Bool, attached: Bool) -> String {
        [owned ? "owned" : "unowned", "\(state)", activated ? "active" : "inactive", failed ? "failed" : nil,
         attached ? "attached" : nil].compactMap { $0 }.joined(separator: "-")
    }
}

extension HvfSessionInputRouter {
    func routeDiagnostic(attached: Bool) -> String {
        HvfInputRouteDiagnostic.label(owned: eligible, state: state, activated: activated, failed: failed, attached: attached)
    }
}

extension HvfSessionInputDriver {
    func routeDiagnostic(attached: Bool) -> String { router.routeDiagnostic(attached: attached) }
}
