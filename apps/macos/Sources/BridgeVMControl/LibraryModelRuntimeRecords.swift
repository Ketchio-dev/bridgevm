import Foundation

extension LibraryModel {
    /// Existing handles only: no session factory, process scan, or guest effect.
    func runtimeRecords(slug: String) throws -> [HvfRuntimeSessionRecord] {
        var records: [HvfRuntimeSessionRecord] = []
        var seen: Set<ObjectIdentifier> = []
        func append(_ record: HvfRuntimeSessionRecord) throws {
            guard seen.insert(ObjectIdentifier(record.session)).inserted else { return }
            guard records.count < NativeRuntimeCodec.maximumSessions else {
                throw NativeRuntimeError.snapshotUnavailable
            }
            records.append(record)
        }
        if let record = hvfRuntimeSessions.existingRecord(slug: slug) { try append(record) }
        for record in retainedControlStore.records {
            guard case let .runtime(config, session) = record.descriptor, config.slug == slug else { continue }
            try append(HvfRuntimeSessionRecord(sourceConfig: config, session: session))
        }
        return records
    }
}
