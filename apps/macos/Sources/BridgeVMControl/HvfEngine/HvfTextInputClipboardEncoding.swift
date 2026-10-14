import Foundation

extension HvfTextInputPlan {
    /// Shared by immediate delivery and deferred admission without allocating HID chunks.
    static func clipboardBase64(for value: String) -> String? {
        guard !value.utf8.allSatisfy({ (0x20...0x7e).contains($0) }) else { return nil }
        return Data(value.utf8).base64EncodedString()
    }
}
