import XCTest
@testable import BridgeVMControl

final class NativeCLIInstallTransportTests: XCTestCase {
    func testStatusUsesTheOwnedSocketAndValidatesInstallResponse() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("install-cli-transport-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let config = VMConfig(id: "windows", name: "windows", displayName: "Windows",
            backendKind: "hvf-engine", bootMode: "windows-hvf",
            bundlePath: root.appendingPathComponent("windows/bundle").path,
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: "windows", displayWidth: 1280,
            displayHeight: 720, installPending: true)
        XCTAssertTrue(VMLibrary.save(config, rootURL: root))
        let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: false)
        let owner = try NativeRuntimeOwner(library: library)
        defer { owner.close() }
        let app = UUID().uuidString
        try owner.start(installHandler: { request, _ in
            let observation = NativeInstallObservation(operationID: UUID().uuidString,
                expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
                phase: .preparingPlan, acceptedUptime: 1, workerPending: true,
                sessionRunning: false, canCancel: true, logTail: [], failure: nil)
            return .init(schema: NativeInstallControlCodec.responseSchema,
                scope: NativeInstallControlCodec.scope, requestID: request.requestID,
                library: library.identity, vmID: request.vmID, appInstanceID: app,
                expectedSavedConfigurationDigest: request.expectedSavedConfigurationDigest,
                requestedOperationID: nil, disposition: .status,
                observation: observation, refusal: nil)
        }, handler: { request in
            .init(schema: NativeRuntimeCodec.responseSchema, requestID: request.requestID,
                library: library.identity, vmID: request.vmID, appInstanceID: app,
                observedAt: ProcessInfo.processInfo.systemUptime,
                scope: NativeRuntimeCodec.scope, sessions: [])
        })

        let result = await Task.detached {
            NativeCLIInstall.run(rootURL: root, id: config.slug, operation: .installStatus)
        }.value
        XCTAssertTrue(result.complete)
        XCTAssertEqual(result.appInstanceID, app)
        XCTAssertEqual(result.disposition, NativeInstallControlResponse.Disposition.status)
        XCTAssertEqual(result.observation?.phase, .preparingPlan)
        XCTAssertNil(result.requestedOperationID)
        XCTAssertNil(result.unavailableReason)
    }
}
