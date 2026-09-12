import Foundation

@main
struct InputReceiptTranscriptContract {
    static func main() throws {
        guard CommandLine.arguments.count == 2 else { fatalError("expected transcript path") }
        let transcript = try String(contentsOfFile: CommandLine.arguments[1], encoding: .utf8)
        for value in ["private-input-fixture", "ctrl+v"] {
            guard !transcript.contains(value),
                  !transcript.contains(Data(value.utf8).base64EncodedString()) else {
                fatalError("input payload leaked into native transcript")
            }
        }
        let lines = transcript.components(separatedBy: .newlines)
        let id = UUID(uuidString: "01234567-89AB-CDEF-0123-456789ABCDEF")!
        let now = Date(timeIntervalSince1970: 20_000)
        for name in ["normal", "chunked", "key", "missing-begin"] {
            let starts = lines.indices.filter { lines[$0] == "BEGIN_BV_INPUT_FIXTURE \(name)" }
            let ends = lines.indices.filter { lines[$0] == "END_BV_INPUT_FIXTURE \(name)" }
            guard starts.count == 1, ends.count == 1, starts[0] < ends[0] else {
                fatalError("missing or ambiguous native fixture frame")
            }
            let event: HvfOrderedInputQueue.Event = name == "key" ? .key("ctrl+v") : .text("private-input-fixture")
            guard var request = HvfUnicodeInputRequest(event: event, now: now, id: id) else {
                fatalError("fixture request rejected")
            }
            let expected: HvfUnicodeInputRequest.Outcome = name == "missing-begin" ? .failed(.guestRejected) : .inserted
            let body = Array(lines[(starts[0] + 1)..<ends[0]])
            guard request.consume(lines: body, now: now) == expected else {
                fatalError("Rust receipt and Swift consumer disagree")
            }
        }
        print("PASS: 4 native-to-Swift receipt cases; input payloads absent; no guest input injected")
    }
}
