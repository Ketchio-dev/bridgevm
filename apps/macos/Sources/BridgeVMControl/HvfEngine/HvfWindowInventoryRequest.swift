import Foundation
import BridgeVMWindowProtocol

enum HvfWindowInventoryError: Error, Equatable {
    case expired, restarted, guestError, malformedRecord, duplicateHandle, resourceLimit
}

struct HvfWindowInventoryRequest {
    static let maximumRecords = 4096
    static let maximumBytes = 4 * 1024 * 1024
    let command: String
    private let prefix: String
    private let deadline: Date
    private var records: [GuestWindowRecord] = []
    private var handles: Set<String> = []
    private var bytes = 0
    private var finished = false

    init(now: Date, id: UUID = UUID()) {
        command = "WINLIST " + id.uuidString
        prefix = "BVAGENT " + command + " "
        deadline = now.addingTimeInterval(30)
    }

    mutating func consume(
        lines: [String], now: Date
    ) -> Result<[GuestWindowRecord], HvfWindowInventoryError>? {
        guard !finished else { return nil }
        guard now < deadline else { return finish(.failure(.expired)) }
        if lines.contains(where: { $0.hasPrefix("BVAGENT READY ") || $0.hasPrefix("BVAGENT re-READY ")
            || $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI SYSTEM_RESET:")
            || $0.hasPrefix("PSCI_SYSTEM_RESET") }) { return finish(.failure(.restarted)) }
        for raw in lines {
            let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
            guard line.hasPrefix(prefix) else { continue }
            let payload = String(line.dropFirst(prefix.count))
            if payload == "WINEND" { return finish(.success(records)) }
            if payload == "-> ERR WINLIST" || payload.hasPrefix("-> ERR WINLIST ") {
                return finish(.failure(.guestError))
            }
            guard records.count < Self.maximumRecords,
                line.utf8.count <= Self.maximumBytes - bytes else {
                return finish(.failure(.resourceLimit))
            }
            bytes += line.utf8.count
            guard let record = GuestWindowRecord(protocolLine: payload) else {
                return finish(.failure(.malformedRecord))
            }
            guard handles.insert(record.id).inserted else {
                return finish(.failure(.duplicateHandle))
            }
            records.append(record)
        }
        return nil
    }

    private mutating func finish(
        _ result: Result<[GuestWindowRecord], HvfWindowInventoryError>
    ) -> Result<[GuestWindowRecord], HvfWindowInventoryError> {
        finished = true
        records.removeAll()
        handles.removeAll()
        return result
    }
}
