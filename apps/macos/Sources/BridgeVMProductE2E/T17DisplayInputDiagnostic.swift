import ApplicationServices
import Foundation

/// The display surface's own account of the presses it received. The app's
/// HvfFramebufferView writes it to the surface's accessibility help, and a failed
/// click carries it, so the lane retains where the click went (T17 r54/r55 lost it).
extension T17UIControlling {
    func displayInputDiagnostic() -> String {
        guard let accessibility = self as? T17Accessibility else { return "" }
        return "; display=" + (T17DisplayInputDiagnostic.help(pid: accessibility.pid) ?? "unreadable")
    }
}

enum T17DisplayInputDiagnostic {
    /// `presses=0` when the surface exists but never recorded a press.
    static func help(pid: pid_t) -> String? {
        let application = AXUIElementCreateApplication(pid)
        guard let nodes = try? T17AccessibilityTree.nodes(application, limit: 12_000),
              let surface = nodes.first(where: {
                  (try? T17AccessibilityTree.attribute($0, kAXIdentifierAttribute as String)) as? String == T17DisplayClick.surface
              }) else { return nil }
        let help = (try? T17AccessibilityTree.attribute(surface, kAXHelpAttribute as String)) as? String
        return help.map(sanitized) ?? "presses=0"
    }

    /// Only the app's own vocabulary, bounded, so the failure detail stays one short token list.
    static func sanitized(_ text: String) -> String {
        String(text.filter { $0.isASCII && ($0.isLetter || $0.isNumber || " =-".contains($0)) }.prefix(80))
    }
}
