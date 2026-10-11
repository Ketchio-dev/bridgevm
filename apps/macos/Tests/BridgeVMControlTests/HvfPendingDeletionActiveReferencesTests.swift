import XCTest
@testable import BridgeVMControl

@MainActor
final class HvfPendingDeletionActiveReferencesTests: XCTestCase {
    func testAcceptedInstallStillProtectsSourceAfterItsRegistrationDisappears() throws {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let target = fixture.config("target", pending: true)
        let consumer = fixture.config("consumer", pending: true)
        fixture.save(target); fixture.save(consumer)
        let request = HvfWindowsInstallRequest(isoPath: target.bundlePath + "/disks/installer.iso",
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: consumer.bundlePath))
        let library = fixture.library()
        fixture.cacheFake(in: library, config: target)
        let session = library.windowsInstallSession(for: consumer)
        session.attemptActive = true
        session.transition(to: .preparingSource)
        defer { session.finish(.cancelled) }
        try FileManager.default.removeItem(at: fixture.registration(consumer))
        library.reload()
        XCTAssertTrue(library.retainedControlRecords.contains { $0.descriptor.config.slug == consumer.slug })
        fixture.assertRefused([.requestDeletion, .deletion], config: target, library: library)
        XCTAssertEqual(fixture.queue.count, 0)
    }

    func testInactiveRetainedInstallDoesNotBlockPendingDeletion() throws {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let target = fixture.config("target", pending: true)
        let consumer = fixture.config("consumer", pending: true)
        fixture.save(target); fixture.save(consumer)
        let request = HvfWindowsInstallRequest(isoPath: target.bundlePath + "/disks/installer.iso",
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: consumer.bundlePath))
        let library = fixture.library()
        fixture.cacheFake(in: library, config: target)
        let session = library.windowsInstallSession(for: consumer)
        session.attemptActive = true
        session.transition(to: .preparingSource)
        try FileManager.default.removeItem(at: fixture.registration(consumer))
        library.reload()
        session.finish(.cancelled)
        XCTAssertNil(library.pendingDeletionReferenceRefusal(for: target))
        fixture.assertAccepted(.deletion, config: target, library: library)
    }

    func testAcceptedInstallKeepsOriginalInputWhenSavedRequestChanges() throws {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let target = fixture.config("target", pending: true)
        let consumer = fixture.config("consumer", pending: true)
        fixture.save(target); fixture.save(consumer)
        var request = HvfWindowsInstallRequest(isoPath: target.bundlePath + "/disks/installer.iso",
            isoSHA256: String(repeating: "a", count: 64), diskGiB: 64, injectViogpu3d: false)
        XCTAssertTrue(request.save(bundlePath: consumer.bundlePath))
        let library = fixture.library()
        fixture.cacheFake(in: library, config: target)
        let session = library.windowsInstallSession(for: consumer)
        session.attemptActive = true
        session.transition(to: .preparingSource)
        defer { session.finish(.cancelled) }
        request.isoPath = fixture.root.appendingPathComponent("new.iso").path
        XCTAssertTrue(request.save(bundlePath: consumer.bundlePath))
        fixture.assertRefused([.requestDeletion, .deletion], config: target, library: library)
        XCTAssertEqual(fixture.queue.count, 0)
    }
}
