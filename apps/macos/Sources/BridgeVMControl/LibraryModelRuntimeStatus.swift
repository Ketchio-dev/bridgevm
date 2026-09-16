import Foundation

extension LibraryModel {
    /// Reads existing handles only, including removed registrations retained for control.
    func runtimeObservations(slug: String, requestedConfigurationIdentity: String?) throws
        -> [NativeRuntimeSessionObservation] {
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
        return try records.map { record in
            let accepted = try record.sourceConfig.map { try NativeRuntimeConfigurationIdentity.digest(config: $0) }
            let match: NativeRuntimeSessionObservation.ConfigurationMatch
            if let accepted, let requestedConfigurationIdentity {
                match = accepted == requestedConfigurationIdentity ? .same : .different
            } else { match = .unknown }
            return record.session.runtimeObservation().nativeStatus(acceptedDigest: accepted, match: match)
        }
    }
}

private extension HvfRuntimeObservation {
    func nativeStatus(acceptedDigest: String?, match: NativeRuntimeSessionObservation.ConfigurationMatch)
        -> NativeRuntimeSessionObservation {
        let ownership: NativeRuntimeSessionObservation.Ownership
        if ownedProcessIdentity != nil { ownership = .owned }
        else if hasRetainedAttachment { ownership = .attachedObservation }
        else if lastOwnedExit != nil { ownership = .ownedExitObserved }
        else { ownership = .notObserved }
        let state: String
        switch connectionState {
        case .stopped: state = "stopped"
        case .booting: state = "booting"
        case .connected: state = "connected"
        case .stopping: state = "stopping"
        case .timedOut: state = "timedOut"
        }
        let exit = lastOwnedExit.map { value in
            let reason: String
            switch value.reason {
            case .exit: reason = "exit"
            case .uncaughtSignal: reason = "uncaughtSignal"
            case .unknown: reason = "unknown"
            }
            return NativeRuntimeExitObservation(process: value.identity.nativeStatus, reason: reason, status: value.status)
        }
        return NativeRuntimeSessionObservation(ownership: ownership,
            connectionState: ownership == .notObserved ? nil : state,
            acceptedConfigurationDigest: acceptedDigest, configurationMatch: match,
            ownedProcess: ownedProcessIdentity?.nativeStatus, lastOwnedExit: exit)
    }
}

private extension HvfOwnedRuntimeIdentity {
    var nativeStatus: NativeRuntimeProcessObservation {
        NativeRuntimeProcessObservation(token: token.uuidString, processID: processID)
    }
}
