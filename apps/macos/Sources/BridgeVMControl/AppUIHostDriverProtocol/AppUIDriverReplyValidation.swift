#if (DEBUG && BRIDGEVM_APP_UI_HOST) || BRIDGEVM_APP_UI_DRIVER
import Foundation

extension AppUIDriverValidation {
    static func reply(_ value: AppUIDriverReply, request: AppUIDriverRequest,
                      session: AppUIDriverSession, requestSHA256: String) throws {
        guard value.schemaVersion == 1, value.kind == "native-app-ui-driver-reply",
              hexDigest(requestSHA256), value.requestSHA256 == requestSHA256,
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
        let mutation = AppUIDriverOperationPolicy.isMutation(request.operation)
        let fields: Set<String> = request.operation == .createForm && value.values?.commitVisible == false
            ? ["commitVisible"] : AppUIDriverOperationPolicy.fields(for: request.operation)
        guard value.outcome == (mutation ? .performed : .observed),
              value.failureCode == nil, value.axError == nil,
              (value.values?.presentFields ?? []) == fields,
              (value.values != nil) == !fields.isEmpty else { throw AppUIDriverFailure.invalidReply }
        if let values = value.values {
            for string in [values.isoSelection, values.payloadSelection, values.manifestSelection, values.searchValue].compactMap({ $0 }) {
                guard string.utf8.count <= 1024, !string.contains("\0") else { throw AppUIDriverFailure.protocolLimit }
            }
            if request.operation == .setSearchIndigo && values.searchValue != "Indigo" {
                throw AppUIDriverFailure.textReadbackMismatch
            }
        }
    }
}
#endif
