import Foundation

extension NativeRuntimeCodec {
    static func validate(_ session: NativeRuntimeSessionObservation, saved: NativeRuntimeSavedConfiguration) throws {
        if let digest = session.acceptedConfigurationDigest, !validDigest(digest) {
            throw NativeRuntimeError.invalidMessage
        }
        let expectedMatch: NativeRuntimeSessionObservation.ConfigurationMatch
        if let accepted = session.acceptedConfigurationDigest, let requested = saved.digest {
            expectedMatch = accepted == requested ? .same : .different
        } else { expectedMatch = .unknown }
        guard session.configurationMatch == expectedMatch else { throw NativeRuntimeError.invalidMessage }
        if let process = session.ownedProcess { try validate(process) }
        if let exit = session.lastOwnedExit {
            try validate(exit.process)
            guard ["exit", "uncaughtSignal", "unknown"].contains(exit.reason) else { throw NativeRuntimeError.invalidMessage }
        }
        switch session.ownership {
        case .owned:
            guard session.ownedProcess != nil else { throw NativeRuntimeError.invalidMessage }
        case .attachedObservation:
            guard session.ownedProcess == nil else { throw NativeRuntimeError.invalidMessage }
        case .ownedExitObserved:
            guard session.ownedProcess == nil, session.lastOwnedExit != nil else { throw NativeRuntimeError.invalidMessage }
        case .notObserved:
            guard session.ownedProcess == nil, session.lastOwnedExit == nil,
                  session.connectionState == nil else { throw NativeRuntimeError.invalidMessage }
        }
        if session.ownership != .notObserved {
            guard let state = session.connectionState,
                  ["stopped", "booting", "connected", "stopping", "timedOut"].contains(state) else {
                throw NativeRuntimeError.invalidMessage
            }
        }
    }

    private static func validate(_ process: NativeRuntimeProcessObservation) throws {
        guard process.processID > 0, validUUID(process.token) else { throw NativeRuntimeError.invalidMessage }
    }
}
