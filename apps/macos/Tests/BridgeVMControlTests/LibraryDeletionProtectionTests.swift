import XCTest
@testable import BridgeVMControl

@MainActor
final class LibraryDeletionProtectionTests: XCTestCase {
    func testOnlyOwnHVFDeletionRequiresNativeMediaLease() {
        let fixture = HvfRuntimeLibraryActionFixture()
        var config = fixture.config()
        XCTAssertEqual(LibraryDeletionProtection.mode(for: config), .nativeMediaLease)

        config.backendKind = BackendKind.fastVZ.rawValue
        XCTAssertEqual(LibraryDeletionProtection.mode(for: config), .registration)
        config.backendKind = BackendKind.qemuCompat.rawValue
        XCTAssertEqual(LibraryDeletionProtection.mode(for: config), .registration)
    }
}
