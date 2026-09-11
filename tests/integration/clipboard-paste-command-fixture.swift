import Foundation

@main
struct ClipboardPasteCommandFixture {
    static func main() throws {
        let inputs = ["plain text", "\u{D55C}\u{AE00}", "'\";$() & | < > `", "first\r\nsecond\tthird"]
        let fixtures = inputs.map { text -> [String: String] in
            let request = HvfClipboardPaste(
                base64: Data(text.utf8).base64EncodedString(), now: Date(timeIntervalSince1970: 0)
            )
            return ["text": text, "command": request.command, "marker": request.marker]
        }
        FileHandle.standardOutput.write(try JSONSerialization.data(withJSONObject: fixtures))
    }
}
