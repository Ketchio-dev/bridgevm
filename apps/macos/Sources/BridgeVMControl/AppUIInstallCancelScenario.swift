#if DEBUG && BRIDGEVM_APP_UI_HOST
import AppKit

@MainActor
enum AppUIInstallCancelScenario {
    static func requested(environment: [String: String] = ProcessInfo.processInfo.environment) throws -> Bool {
        guard let value = environment["BRIDGEVM_APP_UI_INSTALL_CANCEL_MODE"] else { return false }
        guard value == "1", environment["BRIDGEVM_APP_UI_DRIVER_MODE"] == "1" else {
            throw AppUIDriverFailure.invalidRequest
        }
        return true
    }

    static func run(_ host: AppUIHost, fixture: AppUIInstallCancelFixture,
                    window: NSWindow, content: NSView) async throws {
        guard let capture = host.installCancelCapture else { throw AppUIDriverFailure.invalidSession }
        window.setAccessibilityIdentifier(AppUIDriverConstants.mainWindowIdentifier)
        _ = try await fixture.prepared()
        let client = try await AppUIInstallCancelClient.connect(host, window: window, content: content,
            deadline: ProcessInfo.processInfo.systemUptime + 5)
        for phase in AppUIInstallCancelPhase.allCases {
            if phase == .cancelled { fixture.gate.release() }
            let deadline = ProcessInfo.processInfo.systemUptime + 5
            if let mutation = AppUIInstallCancelOperationPolicy.mutation(for: phase) {
                try await client.perform(mutation, phase: phase, deadline: deadline)
            }
            try await AppUIInstallCancelClient.wait(phase.rawValue, deadline: deadline) {
                _ = try await client.perform(.readState, phase: phase, deadline: deadline)
                guard let reply = client.lastReply,
                      AppUIInstallCancelOperationPolicy.completes(phase, reply: reply) else { return false }
                return try fixture.matches(phase)
            }
            try capture.capture(phase, window: window, content: content,
                                                   readSequence: client.lastReadSequence)
        }
    }
}
#endif
