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
    /// The wire's session cap applies only to the status response (runtimeObservations).
    func runtimeRecords(slug: String) -> [HvfRuntimeSessionRecord] {
        var seen: Set<ObjectIdentifier> = []
        let retained = retainedControlStore.records.compactMap { record -> HvfRuntimeSessionRecord? in
            guard case let .runtime(config, session) = record.descriptor, config.slug == slug else { return nil }
            return HvfRuntimeSessionRecord(sourceConfig: config, session: session)
        }
        return ([hvfRuntimeSessions.existingRecord(slug: slug)].compactMap { $0 } + retained)
            .filter { seen.insert(ObjectIdentifier($0.session)).inserted }
    }
}
