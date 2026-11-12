import Foundation

extension LibraryModel {
    /// Every runtime session the app holds, retained ones included, each once.
    func appRuntimeSessions() -> [HvfEngineSession] {
        var seen: Set<ObjectIdentifier> = []
        let retained = retainedControlStore.records.compactMap { record -> HvfEngineSession? in
            guard case let .runtime(_, session) = record.descriptor else { return nil }
            return session
        }
        return (hvfRuntimeSessions.sessions + retained).filter { seen.insert(ObjectIdentifier($0)).inserted }
    }

    func appTerminationSessions() -> [ControlAppTerminationSession] {
        let retained = retainedControlStore.records.compactMap { record -> HvfWindowsInstallSession? in
            guard case let .install(_, session) = record.descriptor else { return nil }
            return session
        }
        var seen: Set<ObjectIdentifier> = []
        let installs = (windowsInstallSessions.sessions + retained).filter { seen.insert(ObjectIdentifier($0)).inserted }
        return appRuntimeSessions().map { $0 as ControlAppTerminationSession }
            + installs.map { $0 as ControlAppTerminationSession }
    }

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
