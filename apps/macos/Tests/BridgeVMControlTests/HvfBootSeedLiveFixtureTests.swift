import Foundation
import XCTest

final class HvfBootSeedLiveFixtureTests: XCTestCase {
    private func fixture() throws -> [String: String] {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let disk = root.appendingPathComponent("disk").path
        let template = root.appendingPathComponent("template").path
        try Data("disk-unchanged".utf8).write(to: URL(fileURLWithPath: disk))
        try Data("template-unchanged".utf8).write(to: URL(fileURLWithPath: template))
        return ["BRIDGEVM_BOOT_SEED_LIVE_DISK": disk, "BRIDGEVM_BOOT_SEED_LIVE_TEMPLATE": template,
                "BRIDGEVM_BOOT_SEED_LIVE_OUTPUT": root.appendingPathComponent("output").path]
    }

    func testExplicitInputsProduceSeparateCopyWithoutChangingInputs() throws {
        let environment = try fixture()
        let (disk, output) = try HvfBootSeedLiveFixture.prepare(environment: environment)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: disk)), Data("disk-unchanged".utf8))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: output)), Data("template-unchanged".utf8))
        let template = try XCTUnwrap(environment["BRIDGEVM_BOOT_SEED_LIVE_TEMPLATE"])
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: template)), Data("template-unchanged".utf8))
    }

    func testExistingOutputAndSymlinkAreNotOverwritten() throws {
        let environment = try fixture()
        let output = try XCTUnwrap(environment["BRIDGEVM_BOOT_SEED_LIVE_OUTPUT"])
        let retained = Data("retained-output".utf8)
        try retained.write(to: URL(fileURLWithPath: output))
        XCTAssertThrowsError(try HvfBootSeedLiveFixture.prepare(environment: environment))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: output)), retained)
        let alias = output + "-alias"
        try FileManager.default.createSymbolicLink(atPath: alias, withDestinationPath: output)
        var linked = environment
        linked["BRIDGEVM_BOOT_SEED_LIVE_OUTPUT"] = alias
        XCTAssertThrowsError(try HvfBootSeedLiveFixture.prepare(environment: linked))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: output)), retained)
    }

    func testPartialAndRelativeConfigurationFailsWithoutCreatingOutput() throws {
        var environment = try fixture()
        let output = try XCTUnwrap(environment["BRIDGEVM_BOOT_SEED_LIVE_OUTPUT"])
        environment["BRIDGEVM_BOOT_SEED_LIVE_DISK"] = nil
        XCTAssertThrowsError(try HvfBootSeedLiveFixture.prepare(environment: environment))
        environment["BRIDGEVM_BOOT_SEED_LIVE_DISK"] = "relative-disk"
        XCTAssertThrowsError(try HvfBootSeedLiveFixture.prepare(environment: environment))
        XCTAssertFalse(FileManager.default.fileExists(atPath: output))
    }
}
