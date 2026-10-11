import Foundation

enum HvfGuestCommand {
    static let maximumBytes = 64 * 1024
    /// First tokens the Rust relay sends raw, not as `RUN <base64>` (`is_raw_verb` and
    /// input verbs); it matches case-sensitively after splitting on Unicode whitespace.
    static let relayProtocolVerbs: Set<String> = [
        "CLIPGET", "CLIPSET", "LS", "LSR", "GET", "PUT", "PUTBEG", "PUTCHUNK", "PUTEND", "DEL", "PING",
        "WINLIST", "WINBOUNDS", "WINFOCUS", "WINCLOSE", "TEXTINPUT", "KEYINPUT", "POINTERINPUT", "INPUTCAPS"
    ]

    /// One control-file line; the diagnostics box and window inventory send verbs on purpose.
    static func normalize(_ value: String) -> Result<String, HvfGuestCommandError> {
        let command = value.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty else { return .failure(.empty) }
        // Scalars, not Characters: "\r\n" is one Character matching neither "\r" nor "\n".
        guard !command.unicodeScalars.contains(where: { $0 == "\n" || $0 == "\r" || $0 == "\0" }) else {
            return .failure(.multiline)
        }
        let byteCount = command.lengthOfBytes(using: .utf8)
        guard byteCount <= maximumBytes else {
            return .failure(.tooLong(actual: byteCount, maximum: maximumBytes))
        }
        return .success(command)
    }

    /// The relay sends a shell command starting with a protocol verb raw, so it never
    /// runs or replies as a command; with no RUN escape on this path, refuse it.
    static func normalizeShell(_ value: String) -> Result<String, HvfGuestCommandError> {
        normalize(value).flatMap { command in
            let first = command.unicodeScalars.split(whereSeparator: \.properties.isWhitespace).first.map(String.init) ?? ""
            return relayProtocolVerbs.contains(first) ? .failure(.protocolVerb(first)) : .success(command)
        }
    }
}
