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

    func testTerminalOperationSurvivesOnlyWhileTheVMIsAPendingInstall() throws {
        let store = NativeInstallOperationStore(), saved = config()
        guard case let .accepted(operation) = store.reserve(vmID: saved.slug, operationID: UUID())
        else { return XCTFail("expected admission") }
        operation.fail("retryable")

        store.reconcile(with: [saved])
        guard case let .success(retained) = store.operation(vmID: saved.slug) else {
            return XCTFail("expected retained terminal result")
        }
        XCTAssertTrue(retained === operation)

        var edited = saved
        edited.displayWidth = 1440
        store.reconcile(with: [edited])
        guard case .success = store.operation(vmID: saved.slug) else {
            return XCTFail("a settings edit keeps the result of the same pending install")
        }

        var installed = saved
        installed.installPending = false
        store.reconcile(with: [installed])
        guard case .failure(.targetUnavailable) = store.operation(vmID: saved.slug) else {
            return XCTFail("expected terminal result removal once the VM is installed")
        }
    }

    func testActiveReservationSurvivesTemporaryLibraryAbsence() throws {
        let store = NativeInstallOperationStore(), saved = config()
        guard case let .accepted(operation) = store.reserve(vmID: saved.slug, operationID: UUID())
        else { return XCTFail("expected admission") }

        store.reconcile(with: [])
        guard case let .success(retained) = store.operation(vmID: saved.slug) else {
            return XCTFail("expected active reservation")
        }
        XCTAssertTrue(retained === operation)

        operation.cancel()
        store.reconcile(with: [])
        guard case .failure(.targetUnavailable) = store.operation(vmID: saved.slug) else {
            return XCTFail("expected terminal removal after target loss")
        }
    }
}
