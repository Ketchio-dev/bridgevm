import Foundation

/// The result bv-product-e2e-tpm.ps1 writes to the share at one boot: exactly eight
/// LF-terminated ASCII lines, at most 512 bytes. Anything else is `result-malformed`.
enum T17TPMGuestResult {
    static let maximumBytes = 512
    static let version = "bridgevm-t17-tpm-pcr-v1"

    static func name(boot: T17TPMBoot, prefix: String) -> String { "t17-tpm-boot\(boot.rawValue)-\(prefix).txt" }

    /// Reads at most `maximumBytes` from a regular, non-symlink file.
    static func read(_ url: URL, nonce: String, boot: T17TPMBoot) -> Result<T17TPMBootRecord, T17TPMReason> {
        // attributesOfItem neither follows a symlink nor caches, so each poll sees the file anew.
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path) else { return .failure(.resultMissing) }
        guard attributes[.type] as? FileAttributeType == .typeRegular,
              let size = (attributes[.size] as? NSNumber)?.intValue, size > 0, size <= maximumBytes,
              let handle = FileHandle(forReadingAtPath: url.path) else { return .failure(.resultUnsafe) }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: maximumBytes + 1), data.count <= maximumBytes else {
            return .failure(.resultUnsafe)
        }
        return parse(data, nonce: nonce, boot: boot)
    }

    static func parse(_ data: Data, nonce: String, boot: T17TPMBoot) -> Result<T17TPMBootRecord, T17TPMReason> {
        guard data.count <= maximumBytes, data.last == 0x0a, data.allSatisfy({ $0 >= 0x20 && $0 < 0x7f || $0 == 0x0a }) else {
            return .failure(.resultMalformed)
        }
        let lines = String(decoding: data.dropLast(), as: UTF8.self).components(separatedBy: "\n")
        let keys = ["nonce", "boot", "tpm_present", "tpm_ready", "pcr_read", "pcr0_sha256", "pcr7_sha256"]
        guard lines.count == 8, lines[0] == version else { return .failure(.resultMalformed) }
        var fields: [String: String] = [:]
        for (key, line) in zip(keys, lines.dropFirst()) {
            guard line.hasPrefix(key + "=") else { return .failure(.resultMalformed) }
            fields[key] = String(line.dropFirst(key.count + 1))
        }
        guard fields["nonce"] == nonce, fields["boot"] == String(boot.rawValue),
              let present = flag(fields["tpm_present"]), let ready = flag(fields["tpm_ready"]),
              let failure = readStatus(fields["pcr_read"]),
              let pcr0 = digest(fields["pcr0_sha256"]), let pcr7 = digest(fields["pcr7_sha256"]),
              (failure == nil) == (pcr0 != nil), (pcr0 != nil) == (pcr7 != nil) else {
            return .failure(.resultMalformed)
        }
        let reason = failure ?? (present == nil || ready == nil ? .tpmQueryFailed : nil)
        return .success(T17TPMBootRecord(boot: boot, reason: reason, tpmPresent: present, tpmReady: ready,
                                         pcr0SHA256: pcr0, pcr7SHA256: pcr7))
    }

    /// `.some(nil)` is the guest's "unknown"; nil is malformed.
    private static func flag(_ value: String?) -> Bool?? {
        switch value {
        case "true": return .some(true)
        case "false": return .some(false)
        case "unknown": return .some(nil)
        default: return nil
        }
    }

    /// `.some(nil)` is "ok"; a guest failure code is kept; nil is malformed.
    private static func readStatus(_ value: String?) -> T17TPMReason?? {
        if value == "ok" { return .some(nil) }
        guard let reason = value.flatMap(T17TPMReason.init(rawValue:)), T17TPMReason.guestReported.contains(reason) else {
            return nil
        }
        return .some(reason)
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
