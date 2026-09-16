import Foundation

/// Bounded O(1) fold of the exact retained runner's events; it never discovers processes.
struct HvfOwnedRuntimeLedger {
    let identity: HvfOwnedRuntimeIdentity
    let manifestSHA256: String
    private(set) var ready = false
    private(set) var initialHelper: HvfOwnedRuntimeChild?
    private(set) var complete: HvfOwnedRuntimeComplete?
    private(set) var stopOperationID: UUID?
    private(set) var stopAcknowledged = false
    private var sequence: UInt64 = 0
    private var helper = HvfOwnedRuntimeRole()
    private var swtpm = HvfOwnedRuntimeRole()

    init(identity: HvfOwnedRuntimeIdentity, manifestSHA256: String) {
        self.identity = identity; self.manifestSHA256 = manifestSHA256
    }

    mutating func admitStop(operationID: UUID) throws {
        guard stopOperationID == nil || stopOperationID == operationID else {
            throw HvfOwnedRuntimeProtocolError.invalidLifecycle
        }
        stopOperationID = operationID
    }

    mutating func accept(_ event: HvfOwnedRuntimeEvent) throws {
        var candidate = self
        try candidate.apply(event)
        self = candidate
    }

    private mutating func apply(_ event: HvfOwnedRuntimeEvent) throws {
        try HvfOwnedRuntimeEventValidation.validate(event)
        guard event.runToken == identity.token.uuidString.lowercased(), event.runnerPID == identity.processID else {
            throw HvfOwnedRuntimeProtocolError.invalidIdentity
        }
        guard complete == nil, sequence < UInt64.max, event.sequence == sequence + 1 else {
            throw HvfOwnedRuntimeProtocolError.invalidSequence
        }
        switch event.kind {
        case "ready":
            guard !ready, event.ready?.manifestSHA256 == manifestSHA256 else { throw invalid }
            ready = true
        case "stopAck":
            guard !stopAcknowledged, let stopOperationID,
                  event.stopAck?.operationID == stopOperationID.uuidString.lowercased() else { throw invalid }
            stopAcknowledged = true
        case "childStarted":
            guard ready, let child = event.child, child.pid != identity.processID else { throw invalid }
            let other = child.role == "helper" ? swtpm.live : helper.live
            guard other?.pid != child.pid else { throw invalid }
            if child.role == "helper" {
                try helper.start(child)
                if child.generation == 0 { initialHelper = child }
            } else {
                try swtpm.start(child)
            }
        case "childReaped":
            guard ready, let child = event.child else { throw invalid }
            if child.role == "helper" { try helper.reap(child) } else { try swtpm.reap(child) }
        case "cleanupUnconfirmed":
            guard let value = event.unconfirmed else { throw invalid }
            let live = value.role == "helper" ? helper.live : swtpm.live
            guard live?.pid == value.pid, live?.generation == value.generation else { throw invalid }
        case "complete":
            guard let value = event.complete, helper.live == nil, swtpm.live == nil,
                  value.helper == helper.summary, value.swtpm == swtpm.summary else { throw invalid }
            guard (value.operationID != nil) == stopAcknowledged,
                  value.cause != "stopRequested" || stopAcknowledged else { throw invalid }
            if let operationID = value.operationID {
                guard operationID == stopOperationID?.uuidString.lowercased(), stopAcknowledged else { throw invalid }
            }
            if ready {
                guard value.mediaLeaseDisposition == "releasedAfterReap" else { throw invalid }
            } else {
                guard value.mediaLeaseDisposition == "notAdmitted", helper.spawned == 0, swtpm.spawned == 0 else { throw invalid }
            }
            complete = value
        default: throw invalid
        }
        sequence = event.sequence
    }

    private var invalid: HvfOwnedRuntimeProtocolError { .invalidLifecycle }
}
