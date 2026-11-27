import Foundation
import XCTest
@testable import BridgeVMControl

/// Tiny images made by hdiutil stand in for the Windows ISO.
final class HvfWindowsInstallISOCheckTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgevm-iso-check-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: root)
    }

    /// Create used to copy and seal any readable file; only Start then said it was the wrong ISO.
    func testCreateRefusesAFileThatIsNotADiscImageBeforeMakingTheVM() throws {
        let iso = root.appendingPathComponent("Windows.iso")
        try Data(count: 4096).write(to: iso)
        let library = root.appendingPathComponent("library", isDirectory: true)
        let request = NativeCLICreateWindowsOptions(name: "VM", isoPath: iso.path, diskGiB: 64, memoryMiB: 6_144,
            cpuCount: 4, resolution: .init(width: 1440, height: 900), networkEnabled: true)
        let options = NativeCLIOptions(command: .createWindows(request), libraryRoot: library,
                                       json: true, showHelp: false)

        XCTAssertThrowsError(try NativeCLI.executeCreateWindows(options)) {
            XCTAssertEqual($0.localizedDescription, HvfWindowsInstallISOCheck.notDiskImage)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: library.path))
    }

    func testX64ImageIsRefusedArmImagePassesAndNeitherStaysAttached() throws {
        let x64 = try image("x64", bootFile: "efi/boot/bootx64.efi")
        // Any letter case, like the script's `find -ipath`.
        let arm = try image("arm", bootFile: "EFI/BOOT/BOOTAA64.EFI")

        XCTAssertEqual(HvfWindowsInstallISOCheck.problem(isoPath: x64.path), HvfWindowsInstallISOCheck.notArm64)
        XCTAssertNil(HvfWindowsInstallISOCheck.problem(isoPath: arm.path))
        XCTAssertFalse(try hdiutil(["info"]).contains(root.lastPathComponent))
    }

    func testAnImageTheUserAlreadyOpenedStaysOpen() throws {
        let arm = try image("arm", bootFile: "efi/boot/bootaa64.efi")
        let mount = root.appendingPathComponent("open", isDirectory: true)
        try FileManager.default.createDirectory(at: mount, withIntermediateDirectories: true)
        _ = try hdiutil(["attach", "-readonly", "-nobrowse", "-mountpoint", mount.path, arm.path])
        defer { _ = try? hdiutil(["detach", mount.path, "-force"]) }

        XCTAssertNil(HvfWindowsInstallISOCheck.problem(isoPath: arm.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: mount.appendingPathComponent("efi/boot/bootaa64.efi").path))
    }

    private func image(_ name: String, bootFile: String) throws -> URL {
        let file = root.appendingPathComponent(name, isDirectory: true).appendingPathComponent(bootFile)
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("boot".utf8).write(to: file)
        let iso = root.appendingPathComponent("\(name).iso")
        _ = try hdiutil(["makehybrid", "-quiet", "-iso", "-joliet", "-udf", "-o", iso.path,
                         root.appendingPathComponent(name).path])
        return iso
    }

    private func hdiutil(_ arguments: [String]) throws -> String {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/hdiutil")
        process.arguments = arguments
        let output = Pipe()
        process.standardOutput = output
        process.standardError = FileHandle.nullDevice
        try process.run()
        let data = output.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            throw NSError(domain: "hdiutil", code: Int(process.terminationStatus),
                          userInfo: [NSLocalizedDescriptionKey: arguments.joined(separator: " ")])
        }
        return String(decoding: data, as: UTF8.self)
    }
}
