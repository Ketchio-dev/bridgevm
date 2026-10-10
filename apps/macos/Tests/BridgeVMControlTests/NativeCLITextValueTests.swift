import Foundation
import XCTest
@testable import BridgeVMControl

final class NativeCLITextValueTests: XCTestCase {
    func testOrdinaryUnicodePathsAndPunctuationRemainReadable() {
        let text = #"/Volumes/VM library/개발 café é العربية עברית 👩‍💻/"quoted"\bundle.vmbridge"#
        XCTAssertEqual(NativeCLITextValue.escaped(text), text)
        XCTAssertEqual(NativeCLITextValue.quoted("개발 Windows 👩‍💻"), "\"개발 Windows 👩‍💻\"")
    }

    func testExactEscapesForEveryUnsafeControlScalar() {
        let values = Array(UInt32(0x00)...0x1f) + Array(UInt32(0x7f)...0x9f)
            + [0x061c, 0x200e, 0x200f] + Array(UInt32(0x2028)...0x202e)
            + Array(UInt32(0x2066)...0x206f)
        let short: [UInt32: String] = [8: #"\b"#, 9: #"\t"#, 10: #"\n"#, 12: #"\f"#, 13: #"\r"#]
        for value in values {
            let scalar = String(Unicode.Scalar(value)!)
            let expected = short[value] ?? String(format: "\\u%04x", value)
            XCTAssertEqual(NativeCLITextValue.escaped(scalar), expected, "U+\(String(value, radix: 16))")
        }
        XCTAssertEqual(NativeCLITextValue.escaped("\u{1b}[2J\u{1b}[HFAKE\r\n\t\u{1b}]0;title\u{7}"),
                       #"\u001b[2J\u001b[HFAKE\r\n\t\u001b]0;title\u0007"#)
    }

    func testQuotedDisplayNameKeepsJSONEscapingAndNeutralizesC1AndBidi() throws {
        let original = "개발 \"name\"\\\n\u{1b}\u{7f}\u{85}\u{61c}\u{202e}\u{2067}"
        let quoted = NativeCLITextValue.quoted(original)
        XCTAssertEqual(quoted, #""개발 \"name\"\\\n\u001b\u007f\u0085\u061c\u202e\u2067""#)
        XCTAssertEqual(try JSONDecoder().decode(String.self, from: Data(quoted.utf8)), original)
    }
}
