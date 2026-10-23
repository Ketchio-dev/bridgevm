import Foundation

/// The result bv-product-e2e-bitlocker.ps1 writes to the share for one phase: exactly twelve
/// LF-terminated ASCII lines, at most 512 bytes. Anything else is `result-malformed`.
enum T19BitLockerGuestResult {
    static let maximumBytes = 512
    static let version = "bridgevm-t19-bitlocker-v1"
    static let editions = ["home", "pro", "enterprise", "education", "server", "other"]
    static let keys = ["nonce", "phase", "outcome", "edition", "volume", "protectors", "pcr_profile", "fve_code",
                       "pcr_read", "pcr0_sha256", "pcr7_sha256"]

    static func name(phase: T19BitLockerPhase, prefix: String) -> String { "t19-bitlocker-\(phase.rawValue)-\(prefix).txt" }

    /// Reads at most `maximumBytes` from a regular, non-symlink file.
    static func read(_ url: URL, nonce: String, phase: T19BitLockerPhase) -> Result<T19BitLockerGuestState, T19BitLockerReason> {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path) else { return .failure(.resultMissing) }
        guard attributes[.type] as? FileAttributeType == .typeRegular,
              let size = (attributes[.size] as? NSNumber)?.intValue, size > 0, size <= maximumBytes,
              let handle = FileHandle(forReadingAtPath: url.path) else { return .failure(.resultUnsafe) }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: maximumBytes + 1), data.count <= maximumBytes else { return .failure(.resultUnsafe) }
        return parse(data, nonce: nonce, phase: phase)
    }

    static func parse(_ data: Data, nonce: String, phase: T19BitLockerPhase) -> Result<T19BitLockerGuestState, T19BitLockerReason> {
        guard data.count <= maximumBytes, data.last == 0x0a, data.allSatisfy({ $0 >= 0x20 && $0 < 0x7f || $0 == 0x0a }) else {
            return .failure(.resultMalformed)
        }
        let lines = String(decoding: data.dropLast(), as: UTF8.self).components(separatedBy: "\n")
        guard lines.count == 12, lines[0] == version else { return .failure(.resultMalformed) }
        var fields: [String: String] = [:]
        for (key, line) in zip(keys, lines.dropFirst()) {
            guard line.hasPrefix(key + "=") else { return .failure(.resultMalformed) }
            fields[key] = String(line.dropFirst(key.count + 1))
        }
        let pcrReads = ["ok"] + T17TPMReason.guestReported.map(\.rawValue)
        guard fields["nonce"] == nonce, fields["phase"] == phase.rawValue,
              let outcome = fields["outcome"], outcome == "ok" || guestOutcome(outcome) != nil,
              let edition = fields["edition"], edition == "unknown" || editions.contains(edition),
              let state = volume(fields["volume"]), let protectors = list(fields["protectors"], maximum: 10),
              let profile = list(fields["pcr_profile"], maximum: 23), let fve = fveCode(fields["fve_code"]),
              let pcrRead = fields["pcr_read"], pcrReads.contains(pcrRead),
              let pcr0 = digest(fields["pcr0_sha256"]), let pcr7 = digest(fields["pcr7_sha256"]),
              (pcrRead == "ok") == (pcr0 != nil), (pcr0 != nil) == (pcr7 != nil) else {
            return .failure(.resultMalformed)
        }
        return .success(T19BitLockerGuestState(
            outcome: guestOutcome(outcome), edition: edition == "unknown" ? nil : edition,
            conversion: state?[0], protection: state?[1], lock: state?[2], percent: state?[3],
            protectors: protectors, pcrProfile: profile, fveCode: fve, pcrRead: pcrRead, pcr0SHA256: pcr0, pcr7SHA256: pcr7))
    }

    private static func guestOutcome(_ value: String) -> T19BitLockerReason? {
        T19BitLockerReason(rawValue: value).flatMap { T19BitLockerReason.guestReported.contains($0) ? $0 : nil }
    }

    /// A decimal without a leading zero, within 0...maximum.
    private static func number(_ text: Substring, maximum: Int) -> Int? {
        guard !text.isEmpty, text.count <= 5, text.utf8.allSatisfy({ (0x30...0x39).contains($0) }),
              text == "0" || !text.hasPrefix("0"), let value = Int(text), value <= maximum else { return nil }
        return value
    }

    /// `.some(nil)` is "unknown"; conversion, protection, lock and percentage within their ranges; nil is malformed.
    private static func volume(_ value: String?) -> [Int]?? {
        guard let value else { return nil }
        if value == "unknown" { return .some(nil) }
        let parts = value.split(separator: ",", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return nil }
        let values = zip(parts, [5, 2, 1, 100]).compactMap { number($0, maximum: $1) }
        return values.count == 4 ? .some(values) : nil
    }

    /// `.some(nil)` is "unknown", `[]` is "none"; distinct ascending values within 0...maximum; nil is malformed.
    private static func list(_ value: String?, maximum: Int) -> [Int]?? {
        guard let value else { return nil }
        if value == "unknown" { return .some(nil) }
        if value == "none" { return .some([]) }
        let parts = value.split(separator: ",", omittingEmptySubsequences: false)
        let values = parts.compactMap { number($0, maximum: maximum) }
        guard values.count == parts.count, values == Array(Set(values)).sorted() else { return nil }
        return .some(values)
    }

    /// `.some(nil)` is "none"; nil is malformed.
    private static func fveCode(_ value: String?) -> Int?? {
        guard let value else { return nil }
        if value == "none" { return .some(nil) }
        return number(Substring(value), maximum: 65535).map { .some($0) }
    }

    /// `.some(nil)` is "absent"; a lowercase SHA-256 is kept; nil is malformed.
    private static func digest(_ value: String?) -> String?? {
        if value == "absent" { return .some(nil) }
        guard let value, value.utf8.count == 64, value.utf8.allSatisfy({ (0x30...0x39).contains($0) || (0x61...0x66).contains($0) }) else {
            return nil
        }
        return .some(value)
    }
}
