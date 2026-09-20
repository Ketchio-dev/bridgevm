import XCTest
@testable import BridgeVMControl

final class HvfDisplayWindowTitleTests: XCTestCase {
    func testSavedLibraryNameWinsWithoutPathInference() {
        XCTAssertEqual(HvfDisplayWindowTitle.resolve(
            libraryName: "Windows 11 ARM64",
            targetDiskPath: "/tmp/not-a-library/disk.raw"), "Windows 11 ARM64")
    }

    func testLegacyProductBundleRecoversVMIdentifier() {
        XCTAssertEqual(HvfDisplayWindowTitle.resolve(
            libraryName: nil,
            targetDiskPath: "/Users/me/Library/Application Support/BridgeVM/vms/windows-11-arm64/bundle.vmbridge/disks/hvf-target.raw"),
            "windows-11-arm64")
    }

    func testWhitespaceNameFallsBackToKnownBundleLayout() {
        XCTAssertEqual(HvfDisplayWindowTitle.resolve(
            libraryName: " \n ",
            targetDiskPath: "/tmp/ubuntu-dev/bundle.vmbridge/disks/root.raw"), "ubuntu-dev")
    }

    func testArbitraryAndRelativePathsDoNotBecomeWindowTitles() {
        for path in ["", "disks/root.raw", "/tmp/disks/root.raw", "/tmp/vm/disk.raw"] {
            XCTAssertEqual(HvfDisplayWindowTitle.resolve(
                libraryName: nil, targetDiskPath: path), "Windows HVF")
        }
    }

    func testNearMatchDoesNotImpersonateProductBundleLayout() {
        XCTAssertEqual(HvfDisplayWindowTitle.resolve(
            libraryName: nil,
            targetDiskPath: "/tmp/vm/bundle.vmbridge/disks-extra/root.raw"), "Windows HVF")
    }
}
