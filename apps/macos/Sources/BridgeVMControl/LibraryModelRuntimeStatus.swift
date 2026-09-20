import Foundation
extension LibraryModel {
    /// Reads existing handles only, including removed registrations retained for control.
    func runtimeObservations(slug: String, requestedConfigurationIdentity: String?) throws
        -> [NativeRuntimeSessionObservation] {
        let records = try runtimeRecords(slug: slug)
        return try records.map { record in
            let accepted = try record.sourceConfig.map { try NativeRuntimeConfigurationIdentity.digest(config: $0) }
            let graphics = record.sourceConfig.map { $0.experimental3DAllowed == true ? NativeRuntimeSessionObservation.GraphicsMode.experimental3D : .basic3DOff }
            let match: NativeRuntimeSessionObservation.ConfigurationMatch
            if let accepted, let requestedConfigurationIdentity {
                match = accepted == requestedConfigurationIdentity ? .same : .different
            } else { match = .unknown }
            return record.session.runtimeObservation().nativeStatus(acceptedDigest: accepted, match: match, acceptedGraphics: graphics)
        }
    }
}
private extension HvfRuntimeObservation {
    func nativeStatus(acceptedDigest: String?, match: NativeRuntimeSessionObservation.ConfigurationMatch, acceptedGraphics: NativeRuntimeSessionObservation.GraphicsMode?)
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
        let graphics: NativeRuntimeSessionObservation.GraphicsMode? = ownership == .owned ? acceptedGraphics : (ownership == .attachedObservation ? .unverified : nil)
        return NativeRuntimeSessionObservation(ownership: ownership,
            connectionState: ownership == .notObserved ? nil : state,
            acceptedConfigurationDigest: acceptedDigest, configurationMatch: match,
            ownedProcess: ownedProcessIdentity?.nativeStatus, lastOwnedExit: exit, graphicsMode: graphics)
    }
}

private extension HvfOwnedRuntimeIdentity {
    var nativeStatus: NativeRuntimeProcessObservation {
        NativeRuntimeProcessObservation(token: token.uuidString, processID: processID)
    }
}
