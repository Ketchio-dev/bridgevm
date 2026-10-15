import Foundation

enum AppUIHostV2Scenario {
    static func publish(mode: AppUIHostV2Mode, root: URL, nonce: String, startedAt: Double,
                        host: AppUIDriverProcessIdentity, driver: AppUIDriverProcessIdentity) throws {
        switch mode {
        case .overview:
            _ = try AppUIDriverMailbox(privateRoot: root).publishSession(AppUIDriverSession(nonce: nonce,
                startedUptime: startedAt, deadlineUptime: startedAt + 90, host: host, driver: driver))
        case .installCancel:
            _ = try AppUIInstallCancelMailbox(privateRoot: root).publishSession(AppUIInstallCancelSession(nonce: nonce,
                startedUptime: startedAt, deadlineUptime: startedAt + 90, host: host, driver: driver))
        }
    }
    static func completed(mode: AppUIHostV2Mode, root: URL) throws -> Bool {
        switch mode {
        case .overview:
            let mailbox = try AppUIDriverMailbox(privateRoot: root)
            guard let driver = try mailbox.readCompletion(), let host = try mailbox.readHostCompletion() else { return false }
            return host.success && driver.success
        case .installCancel:
            let mailbox = try AppUIInstallCancelMailbox(privateRoot: root)
            guard let driver = try mailbox.readCompletion(), let host = try mailbox.readHostCompletion() else { return false }
            return host.success && driver.success
        }
    }
}
