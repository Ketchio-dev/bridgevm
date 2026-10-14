import Foundation

enum AppUIInstallCancelProtocolTestSupport {
    static func session(root: String = "/fixture/app-ui-private") -> AppUIInstallCancelSession {
        let existing = AppUIDriverProtocolTestSupport.session(root: root)
        return AppUIInstallCancelSession(nonce: existing.nonce, startedUptime: existing.startedUptime,
            deadlineUptime: existing.deadlineUptime, host: existing.host, driver: existing.driver)
    }

    static func hash(_ value: AppUIInstallCancelSession) throws -> String {
        AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(value))
    }

    static func request(_ operation: AppUIInstallCancelOperation = .readState,
                        phase: AppUIInstallCancelPhase = .idle, sequence: UInt32 = 1,
                        deadline: Double = 105, session: AppUIInstallCancelSession = session()) throws -> AppUIInstallCancelRequest {
        AppUIInstallCancelRequest(sessionSHA256: try hash(session), nonce: session.nonce, sequence: sequence,
            phase: phase, phaseDeadlineUptime: deadline, window: .main, operation: operation)
    }

    static func reply(_ request: AppUIInstallCancelRequest, values: AppUIInstallCancelValues? = nil,
                      failure: AppUIDriverFailure? = nil,
                      session: AppUIInstallCancelSession = session()) throws -> AppUIInstallCancelReply {
        AppUIInstallCancelReply(sessionSHA256: request.sessionSHA256, nonce: request.nonce, sequence: request.sequence,
            requestSHA256: AppUIDriverCanonicalJSON.digest(try AppUIDriverCanonicalJSON.encode(request)),
            hostPID: session.host.pid, driverPID: session.driver.pid, operation: request.operation,
            outcome: failure != nil ? .refused : request.operation == .readState ? .observed : .performed,
            values: values, failureCode: failure, axError: failure == .axFailure ? -25204 : nil)
    }

    static func values(_ phase: AppUIInstallCancelPhase) -> AppUIInstallCancelValues {
        switch phase {
        case .idle:
            return .init(installVisible: true, startVisible: true, startEnabled: true,
                cancelVisible: false, cancelEnabled: false, stage: "대기", title: "설치 시작")
        case .validating:
            return .init(installVisible: true, startVisible: true, startEnabled: false,
                cancelVisible: true, cancelEnabled: true, stage: "설치 입력 확인", title: "설치 중…")
        case .cancelling:
            return .init(installVisible: true, startVisible: true, startEnabled: false,
                cancelVisible: true, cancelEnabled: false, stage: "취소 중…", title: "설치 중…")
        case .cancelled:
            return .init(installVisible: true, startVisible: true, startEnabled: true,
                cancelVisible: false, cancelEnabled: false, stage: "취소됨", title: "다시 시도")
        case .retryFailed:
            return .init(installVisible: true, startVisible: true, startEnabled: true,
                cancelVisible: false, cancelEnabled: false, stage: "실패", title: "다시 시도",
                failure: AppUIInstallCancelConstants.refusal)
        }
    }

    static func ready(_ session: AppUIInstallCancelSession, trusted: Bool) throws -> AppUIInstallCancelReady {
        AppUIInstallCancelReady(sessionSHA256: try hash(session), nonce: session.nonce, driver: session.driver,
            trusted: trusted, failureCode: trusted ? nil : .accessibilityUntrusted,
            codeRequirement: nil, codeRequirementUnavailableReason: "not-observed-in-pure-test")
    }

    static func completion(_ session: AppUIInstallCancelSession, success: Bool = true,
                           requests: Int = 8, mutations: Int = 3) throws -> AppUIInstallCancelCompletion {
        AppUIInstallCancelCompletion(sessionSHA256: try hash(session), nonce: session.nonce,
            hostPID: session.host.pid, driverPID: session.driver.pid, success: success, failureCode: nil,
            requestsProcessed: requests, mutationsPerformed: mutations)
    }
}
