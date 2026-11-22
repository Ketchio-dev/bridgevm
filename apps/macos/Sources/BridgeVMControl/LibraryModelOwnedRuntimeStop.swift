import Foundation

extension LibraryModel {
    func requestOwnedRuntimeStop(_ request: NativeRuntimeControlRequest) throws -> HvfOwnedStopAdmission {
        guard NativeLibraryReader.isCanonicalID(request.vmID), let token = UUID(uuidString: request.target.runToken),
              let operationID = UUID(uuidString: request.operationID) else { throw NativeRuntimeError.invalidMessage }
        let identity = HvfOwnedRuntimeIdentity(token: token, processID: request.target.processID)
        let matches = try runtimeRecords(slug: request.vmID).filter {
            $0.session.runtimeObservation().ownedProcessIdentity == identity
        }
        guard !matches.isEmpty else { throw NativeRuntimeStopRefused.targetUnavailable }
        guard matches.count == 1, let record = matches.first else { throw NativeRuntimeStopRefused.ambiguousTarget }
        guard let config = record.sourceConfig,
              try NativeRuntimeConfigurationIdentity.digest(config: config) == request.target.acceptedConfigurationDigest
        else { throw NativeRuntimeStopRefused.configurationChanged }
        return record.session.requestOwnedStop(target: identity, operationID: operationID)
    }
}
