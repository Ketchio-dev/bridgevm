import ApplicationServices
import Foundation

@MainActor
enum AppUIInstallCancelDriver {
    static func run(arguments: [String]) -> Int32 {
        let startupDeadline = ProcessInfo.processInfo.systemUptime + 90
        do {
            guard arguments.count == 3, arguments[0] == "--app-ui-install-cancel-driver-v1",
                  arguments[1] == "--private-root",
                  AppUIDriverValidation.canonicalPath(arguments[2]) else {
                throw AppUIDriverFailure.invalidRequest
            }
            let mailbox = try AppUIInstallCancelMailbox(privateRoot: URL(fileURLWithPath: arguments[2], isDirectory: true))
            try AppUIDriverApplication.start(privateRoot: mailbox.privateRoot)
            while ProcessInfo.processInfo.systemUptime < startupDeadline {
                guard try !mailbox.isCancelled() else { throw AppUIDriverFailure.cancelled }
                if let item = try mailbox.readSession() {
                    let session = item.session
                    let service = try AppUIInstallCancelService(mailbox: mailbox, session: session, sha256: item.sha256)
                    do {
                        try AppUIDriverIdentity.verify(session.driver, privateRoot: mailbox.privateRoot, driver: true)
                        try AppUIDriverIdentity.verify(session.host, privateRoot: mailbox.privateRoot, driver: false)
                        let requirement = AppUIDriverIdentity.codeRequirement()
                        // This one observation never asks macOS to show a permission prompt.
                        let trusted = AXIsProcessTrusted()
                        try mailbox.publishReady(AppUIInstallCancelReady(sessionSHA256: item.sha256,
                            nonce: session.nonce, driver: session.driver, trusted: trusted,
                            failureCode: trusted ? nil : .accessibilityUntrusted,
                            codeRequirement: requirement.requirement,
                            codeRequirementUnavailableReason: requirement.unavailable))
                        return trusted ? service.run() : service.finish(success: false, failure: .accessibilityUntrusted)
                    } catch {
                        return service.finish(success: false, failure: AppUIInstallCancelService.failure(error).code)
                    }
                }
                _ = RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05))
            }
            throw AppUIDriverFailure.deadlineExceeded
        } catch {
            FileHandle.standardError.write(Data("app UI driver refused: \(AppUIInstallCancelService.failure(error).code.rawValue)\n".utf8))
            return 2
        }
    }
}
