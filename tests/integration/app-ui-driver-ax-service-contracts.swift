import Foundation
import Darwin

@main
@MainActor
enum AppUIDriverAXServiceContracts {
    typealias T = AppUIDriverProtocolTestSupport
    static func main() {
        do {
            for api in AppUIDriverAXContext.API.allCases {
                let context = AppUIDriverAXContext(api: api, attribute: .windows, scope: .element)
                let error = AppUIDriverAXError(failure: .axFailure, rawValue: -25201, context: context)
                for result in [AppUIDriverService.failure(error), AppUIInstallCancelService.failure(error)] {
                    try T.check(result.code == .axFailure && result.axError == -25201 && result.context == context,
                                "both actual services retain AX error attribution")
                }
            }
            for error: Error in [AppUIDriverFailure.cancelled, AppUIDriverFailure.deadlineExceeded,
                                 AppUIDriverFailure.identityMismatch, NSError(domain: "private text", code: 1)] {
                for result in [AppUIDriverService.failure(error), AppUIInstallCancelService.failure(error)] {
                    try T.check(result.code == (error as? AppUIDriverFailure ?? .internalFailure)
                                && result.axError == nil && result.context == nil, "non-AX errors carry no context or arbitrary text")
                }
            }
            let legacy = AppUIDriverAXError(failure: .axFailure, rawValue: -25204)
            for result in [AppUIDriverService.failure(legacy), AppUIInstallCancelService.failure(legacy)] {
                try T.check(result.code == .axFailure && result.axError == -25204 && result.context == nil, "legacy AX error remains readable")
            }
            print("PASS: \(T.checks) actual service error mapping checks; no service startup or AX query")
        } catch {
            FileHandle.standardError.write(Data("FAIL: AX service error mapping: \(error)\n".utf8)); exit(1)
        }
    }
}
