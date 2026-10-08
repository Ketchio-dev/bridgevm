import XCTest
@testable import BridgeVMControl

final class HvfWindowsInstallDiskSizeTests: XCTestCase {
    func testFileOffsetRepresentationBoundariesWithoutAllocatingDisk() {
        let largest = Int(Int64.max / (1 << 30))
        for invalid in [Int.min, -1, 0, largest + 1, Int.max] {
            XCTAssertNil(HvfWindowsInstallPlan.targetSizeBytes(diskGiB: invalid))
        }
        for valid in [1, 64, 128, largest] {
            XCTAssertEqual(HvfWindowsInstallPlan.targetSizeBytes(diskGiB: valid), UInt64(valid) << 30)
        }
        XCTAssertNotNil(HvfWindowsInstallPlan.diskSizeError(63))
        XCTAssertNil(HvfWindowsInstallPlan.diskSizeError(64))
        XCTAssertEqual(HvfWindowsInstallPlan.diskSizeError(largest + 1),
                       HvfWindowsInstallPlan.unrepresentableDiskSizeMessage)
    }

    func testUnrepresentableSizeIsRejectedBeforeCreatingBundle() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("install-size-" + UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let iso = root.appendingPathComponent("synthetic.iso")
        try Data("synthetic".utf8).write(to: iso)
        let config = VMLibrary.createWindowsHVFInstall(
            name: "Invalid Size", isoPath: iso.path, diskGiB: Int.max,
            injectViogpu3d: false, driverPackageDir: nil, storageDir: root,
            libraryRoot: root, persist: false)
        XCTAssertNil(config)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), ["synthetic.iso"])
    }

    func testUnrepresentablePersistedDiskSizeIsRefusedWithoutTrapping() throws {
        let request = HvfWindowsInstallRequest(
            isoPath: "/absent.iso", isoSHA256: String(repeating: "a", count: 64),
            diskGiB: Int.max, injectViogpu3d: false, driverPackageDir: nil)
        let decoded = try JSONDecoder().decode(
            HvfWindowsInstallRequest.self, from: JSONEncoder().encode(request))
        let plan = HvfWindowsInstallPlan(
            repoRoot: URL(fileURLWithPath: "/absent-recipe"),
            libraryRoot: URL(fileURLWithPath: "/absent-library"),
            bundlePath: "/absent-bundle", slug: "invalid-size", request: decoded)
        XCTAssertNil(plan.freshTargetSizeBytes)
    }
}
