import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallOperationReconcileTests: XCTestCase {
    private func config(_ slug: String = "windows") -> VMConfig {
        VMConfig(id: slug, name: slug, displayName: slug, backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: "/tmp/" + slug + "/bundle",
            runnerPath: "", launchSpecPath: "", handoffPath: "", sshKeyPath: "",
            sshUser: "", leasesPath: "", guestName: slug, displayWidth: 1280,
            displayHeight: 720, installPending: true)
    }

    func testTerminalOperationSurvivesOnlyForExactPendingConfiguration() throws {
        let store = NativeInstallOperationStore(), saved = config()
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: saved)
        guard case let .accepted(operation) = store.reserve(vmID: saved.slug,
            digest: digest, operationID: UUID()) else { return XCTFail("expected admission") }
        operation.fail("retryable")

        store.reconcile(with: [saved])
        guard case let .success(retained) = store.operation(vmID: saved.slug, digest: digest) else {
            return XCTFail("expected retained terminal result")
        }
        XCTAssertTrue(retained === operation)

        var changed = saved
        changed.displayWidth = 1440
        store.reconcile(with: [changed])
        guard case .failure(.targetUnavailable) = store.operation(vmID: saved.slug, digest: digest) else {
            return XCTFail("expected stale terminal result removal")
        }
    }

    func testActiveReservationSurvivesTemporaryLibraryAbsence() throws {
        let store = NativeInstallOperationStore(), saved = config()
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: saved)
        guard case let .accepted(operation) = store.reserve(vmID: saved.slug,
            digest: digest, operationID: UUID()) else { return XCTFail("expected admission") }

        store.reconcile(with: [])
        guard case let .success(retained) = store.operation(vmID: saved.slug, digest: digest) else {
            return XCTFail("expected active reservation")
        }
        XCTAssertTrue(retained === operation)

        operation.cancel()
        store.reconcile(with: [])
        guard case .failure(.targetUnavailable) = store.operation(vmID: saved.slug, digest: digest) else {
            return XCTFail("expected terminal removal after target loss")
        }
    }
}
