import AppKit
import Foundation

@MainActor
enum AppUIHostV2Application {
    static func identity(_ app: NSRunningApplication, expected: AppUIHostV2Ownership,
                         allowTerminatedObservation: Bool = false) throws -> AppUIDriverProcessIdentity {
        guard (allowTerminatedObservation || !app.isTerminated), let date = app.launchDate,
              let bundle = app.bundleURL?.resolvingSymlinksInPath(),
              let executable = app.executableURL?.resolvingSymlinksInPath(),
              bundle.path == expected.bundlePath, executable.path == expected.executablePath,
              app.bundleIdentifier == expected.role.identifier else { throw AppUIDriverFailure.identityMismatch }
        let value = AppUIDriverProcessIdentity(pid: app.processIdentifier,
            launchDate: date.timeIntervalSince1970, bundleIdentifier: expected.role.identifier,
            bundlePath: bundle.path, executablePath: executable.path,
            executableSHA256: try AppUIHostLaunchPaths.digest(executable))
        guard expected.matches(value) else { throw AppUIDriverFailure.identityMismatch }
        return value
    }

    static func configuration(_ role: AppUIHostV2Role, paths: AppUIHostV2Paths) -> NSWorkspace.OpenConfiguration {
        let value = NSWorkspace.OpenConfiguration()
        value.createsNewApplicationInstance = true
        value.allowsRunningApplicationSubstitution = false
        value.addsToRecentItems = false
        value.promptsUserIfNeeded = false
        value.activates = role == .host
        if role == .host {
            value.environment = ["BRIDGEVM_APP_UI_HOST_MODE": "1",
                "BRIDGEVM_APP_UI_HOST_OUTPUT": paths.observations.path, "BRIDGEVM_APP_UI_DRIVER_MODE": "1"]
        } else {
            value.arguments = ["--app-ui-ax-driver", "--private-root", paths.root.path]
        }
        return value
    }
}
