import Foundation

/// The host records of a bound post-footer tail as integer fields, for the private
/// lane result (T17FirstRunHostRecords.swift): the HDA diagnostic records and the display
/// timing record. Each one the host printed becomes an object named after its prefix,
/// `hda poll:` as hda_poll, holding the record's unsigned fields; a three-decimal millisecond
/// field becomes integer microseconds, cbl_ms as cbl_us. product_e2e_host_records.py admits
/// the same names. A record with a value beyond UInt64 is left out. The tail comes from
/// HvfTerminalReport.hostTail, whose grammar admits each record at most once.
enum HvfHostRecordFields {
    static let records = HvfHostTail.diagnosticRecords + [HvfHostTail.displayRecord]
    private static let millis = #"\.[0-9]{3}"#

    static func parse(tail: Data) -> [String: [String: UInt64]] {
        var parsed: [String: [String: UInt64]] = [:]
        for line in String(decoding: tail, as: UTF8.self).split(separator: "\n") {
            guard let record = records.first(where: { line.hasPrefix($0.prefix + " ") }) else { continue }
            let tokens = line.dropFirst(record.prefix.count + 1).split(separator: " ", omittingEmptySubsequences: false)
            let fields = zip(record.fields, tokens).compactMap { field($0, token: $1) }
            if tokens.count == record.fields.count, fields.count == tokens.count {
                parsed[name(record.prefix)] = Dictionary(fields, uniquingKeysWith: { first, _ in first })
            }
        }
        return parsed
    }

    /// `hda CoreAudio timing:` as hda_coreaudio_timing.
    static func name(_ prefix: String) -> String {
        prefix.dropLast().lowercased().replacingOccurrences(of: " ", with: "_")
    }

    /// One `field=value` token: the value, or a three-decimal millisecond value in microseconds.
    private static func field(_ field: String, token: Substring) -> (String, UInt64)? {
        guard token.hasPrefix(field + "=") else { return nil }
        let value = token.dropFirst(field.count + 1)
        guard HvfHostTail.diagnosticFieldValues[field]?.hasSuffix(millis) == true else {
            return number(value).map { (field, $0) }
        }
        let parts = value.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 2, parts[1].count == 3, let whole = number(parts[0]), let thousandths = number(parts[1]) else {
            return nil
        }
        let (micros, overflow) = whole.multipliedReportingOverflow(by: 1000)
        let (total, carry) = micros.addingReportingOverflow(thousandths)
        return overflow || carry ? nil : (field.replacingOccurrences(of: "_ms", with: "_us"), total)
    }

    private static func number(_ digits: Substring) -> UInt64? {
        guard !digits.isEmpty, digits.utf8.allSatisfy({ (48...57).contains($0) }) else { return nil }
        return UInt64(digits)
    }
}
