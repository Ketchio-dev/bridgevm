import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeInstallFileActionAdmissionTests: XCTestCase {
    func testPreparingCLIInstallReservesEveryFileActionBeforeSessionExists() throws {
        for entry in HvfRuntimeLibraryActionEntry.allCases {
            let fixture = HvfRuntimeLibraryActionFixture()
            defer { fixture.clean() }
            let config = fixture.config()
            fixture.save(config)
            let library = fixture.library()
            fixture.cacheFake(in: library, config: config)
            let digest = try NativeRuntimeConfigurationIdentity.digest(config: config)
            guard case let .accepted(operation) = library.windowsInstallSessions.cliOperations.reserve(
                vmID: config.slug, digest: digest, operationID: UUID()) else {
                return XCTFail("Expected CLI preparation reservation")
            }
            XCTAssertEqual(try operation.observation().phase, .preparingPlan)
            XCTAssertNil(library.windowsInstallSessions.record(for: config.slug))
            let before = try fixture.snapshot()
            fixture.assertRefused([entry], config: config, library: library)
            XCTAssertEqual(fixture.queue.count, 0)
            XCTAssertEqual(try fixture.snapshot(), before)
        }
    }

    func testTerminalPreparationCancellationReleasesFileActionAdmission() throws {
        let fixture = HvfRuntimeLibraryActionFixture()
        defer { fixture.clean() }
        let config = fixture.config(pending: true)
        fixture.save(config)
        let library = fixture.library()
        fixture.cacheFake(in: library, config: config)
        let digest = try NativeRuntimeConfigurationIdentity.digest(config: config)
        guard case let .accepted(operation) = library.windowsInstallSessions.cliOperations.reserve(
            vmID: config.slug, digest: digest, operationID: UUID()) else {
            return XCTFail("Expected CLI preparation reservation")
        }
        XCTAssertNotNil(library.libraryActionRefusal(for: config))
        XCTAssertTrue(operation.cancel())
        XCTAssertNil(library.libraryActionRefusal(for: config))
        fixture.assertAccepted(.deletion, config: config, library: library)
    }
}
