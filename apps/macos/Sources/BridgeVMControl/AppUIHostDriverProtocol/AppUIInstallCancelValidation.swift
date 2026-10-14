#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

enum AppUIInstallCancelValidation {
    static func session(_ value: AppUIInstallCancelSession) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-install-cancel-session" else {
            throw AppUIDriverFailure.invalidSession
        }
        try AppUIDriverValidation.session(AppUIDriverSession(nonce: value.nonce,
            startedUptime: value.startedUptime, deadlineUptime: value.deadlineUptime,
            host: value.host, driver: value.driver))
    }

    static func request(_ value: AppUIInstallCancelRequest, session: AppUIInstallCancelSession,
                        sessionSHA256: String, now: Double) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-install-cancel-request",
              AppUIDriverValidation.hexDigest(sessionSHA256), value.sessionSHA256 == sessionSHA256,
              value.nonce == session.nonce, (1...AppUIDriverConstants.maximumSequence).contains(value.sequence),
              now.isFinite, now >= session.startedUptime, now < session.deadlineUptime,
              value.phaseDeadlineUptime.isFinite, value.phaseDeadlineUptime > now,
              value.phaseDeadlineUptime <= session.deadlineUptime, value.phaseDeadlineUptime <= now + 5,
              AppUIInstallCancelOperationPolicy.operations(for: value.phase).contains(value.operation),
              value.window == .main else { throw AppUIDriverFailure.invalidRequest }
    }

    static func ready(_ value: AppUIInstallCancelReady, session: AppUIInstallCancelSession,
                      sessionSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-install-cancel-ready",
              value.sessionSHA256 == sessionSHA256, AppUIDriverValidation.hexDigest(sessionSHA256),
              value.nonce == session.nonce, value.driver == session.driver,
              value.failureCode == (value.trusted ? nil : .accessibilityUntrusted),
              (value.codeRequirement != nil) != (value.codeRequirementUnavailableReason != nil),
              value.codeRequirement.map({ !$0.isEmpty && $0.utf8.count <= 4096 && !$0.contains("\0") }) ?? true,
              value.codeRequirementUnavailableReason.map({ !$0.isEmpty && $0.utf8.count <= 128 && !$0.contains("\0") }) ?? true
        else { throw AppUIDriverFailure.invalidReply }
    }

    static func reply(_ value: AppUIInstallCancelReply, request: AppUIInstallCancelRequest,
                      session: AppUIInstallCancelSession, requestSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-install-cancel-reply",
              AppUIDriverValidation.hexDigest(requestSHA256), value.requestSHA256 == requestSHA256,
              value.sessionSHA256 == request.sessionSHA256, value.nonce == session.nonce,
              value.sequence == request.sequence, value.operation == request.operation,
              value.hostPID == session.host.pid, value.driverPID == session.driver.pid else {
            throw AppUIDriverFailure.invalidReply
        }
        if value.outcome == .refused {
            guard value.failureCode != nil, value.values == nil, value.axError != 0 else {
                throw AppUIDriverFailure.invalidReply
            }
            return
        }
        let mutation = AppUIInstallCancelOperationPolicy.isMutation(request.operation)
        guard value.outcome == (mutation ? .performed : .observed),
              value.failureCode == nil, value.axError == nil,
              (value.values == nil) == mutation else { throw AppUIDriverFailure.invalidReply }
        if let values = value.values {
            guard [values.stage, values.title].allSatisfy({ $0.utf8.count <= 1024 && !$0.contains("\0") }),
                  values.failure == nil || values.failure == AppUIInstallCancelOperationPolicy.syntheticFailure else {
                throw AppUIDriverFailure.protocolLimit
            }
        }
    }
}
#endif
