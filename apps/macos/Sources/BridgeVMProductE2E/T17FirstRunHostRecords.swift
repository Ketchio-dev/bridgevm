import Foundation

/// The first run's host records in a T17 or T19 lane result. The journey archives the
/// run log of its first boot, where the guest played audio and took the keyboard and
/// pointer challenge, as metadata/product-e2e/first-run.log. The records of that log's
/// bound post-footer host tail (HvfHostRecordFields) join the private lane result as
/// `first_run_host_records`. When the host printed none, or the log is missing, unsafe,
/// over 64 MiB or without a bound final report, the key is absent and the lane result
/// is byte-identical to the one before. Nothing gates on the records.
enum T17FirstRunHostRecords {
    static let key = "first_run_host_records"
    static let archive = "first-run.log"

    static func read(bundlePath: String) -> [String: [String: UInt64]] {
        let log = URL(fileURLWithPath: bundlePath).appendingPathComponent("metadata/product-e2e/\(archive)")
        guard let values = try? log.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]),
              values.isRegularFile == true, values.isSymbolicLink != true, (values.fileSize ?? .max) <= 64 * 1024 * 1024,
              let data = try? Data(contentsOf: log, options: [.mappedIfSafe]),
              let tail = HvfTerminalReport.hostTail(in: data) else { return [:] }
        return HvfHostRecordFields.parse(tail: tail)
    }

    static func attach<Lane: Encodable>(_ lane: Lane, bundlePath: String) -> T17HostRecordLane<Lane> {
        T17HostRecordLane(lane: lane, records: read(bundlePath: bundlePath))
    }
}

/// A lane result with the first run's host records added to its own keyed container.
struct T17HostRecordLane<Lane: Encodable>: Encodable {
    let lane: Lane
    let records: [String: [String: UInt64]]

    func encode(to encoder: Encoder) throws {
        try lane.encode(to: encoder)
        guard !records.isEmpty else { return }
        var output = encoder.container(keyedBy: DynamicKey.self)
        try output.encode(records, forKey: .init(T17FirstRunHostRecords.key))
    }
}
