import AppKit

@MainActor
enum AppUIDriverApplication {
    static func start(privateRoot: URL) throws {
        let expected = privateRoot.appendingPathComponent("BridgeVMAppUIDriver.app")
        guard Bundle.main.bundleIdentifier == AppUIDriverConstants.driverBundleIdentifier,
              Bundle.main.bundleURL.resolvingSymlinksInPath().path == expected.path else {
            throw AppUIDriverFailure.identityMismatch
        }
        let application = NSApplication.shared
        if application.activationPolicy() != .accessory { _ = application.setActivationPolicy(.accessory) }
        guard application.activationPolicy() == .accessory else { throw AppUIDriverFailure.internalFailure }
        application.finishLaunching()
    }
}
