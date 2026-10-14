import ApplicationServices
import Foundation

extension AppUIDriverAX {
    func check() throws {
        guard deadline.isFinite, now() < deadline else { throw AppUIDriverFailure.deadlineExceeded }
        guard try !cancelled() else { throw AppUIDriverFailure.cancelled }
    }
    func prepare(_ node: AXUIElement, attribute: String? = nil,
                 scope: AppUIDriverAXContext.Scope = .element) throws {
        try check()
        var observed: pid_t = 0
        try status(calls.getPID(node, &observed), .getPID, attribute: attribute, scope: scope)
        guard observed == pid else { throw AppUIDriverFailure.identityMismatch }
        let remaining = deadline - now()
        guard remaining > 0 else { throw AppUIDriverFailure.deadlineExceeded }
        try status(calls.timeout(node, Float(min(0.25, remaining))), .setMessagingTimeout,
                   attribute: attribute, scope: scope)
        try check()
    }
    func status(_ value: AXError, _ api: AppUIDriverAXContext.API, attribute: String? = nil,
                scope: AppUIDriverAXContext.Scope = .element) throws {
        guard value == .success else {
            throw AppUIDriverAXError(failure: .axFailure, rawValue: value.rawValue,
                context: .init(api: api, attribute: .init(attribute), scope: scope))
        }
    }
}
