import Foundation

/// Escape values, not the renderer's own line and column separators. JSON stays lossless.
enum NativeCLITextValue {
    static func escaped(_ text: String) -> String {
        var result = ""
        for scalar in text.unicodeScalars {
            switch scalar.value {
            case 0x08: result += "\\b"
            case 0x09: result += "\\t"
            case 0x0a: result += "\\n"
            case 0x0c: result += "\\f"
            case 0x0d: result += "\\r"
            case 0x00...0x1f, 0x7f...0x9f, 0x061c, 0x200e...0x200f,
                 0x2028...0x202e, 0x2066...0x206f:
                result += String(format: "\\u%04x", scalar.value)
            default: result.unicodeScalars.append(scalar)
            }
        }
        return result
    }

    static func quoted(_ text: String) -> String {
        guard let data = try? JSONEncoder().encode(text) else { return "\"\"" }
        // JSON already escapes quotes and C0; also neutralize C1 and bidi controls.
        return escaped(String(decoding: data, as: UTF8.self))
    }
}
