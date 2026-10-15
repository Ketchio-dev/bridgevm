import XCTest
@testable import BridgeVMControl

extension FirstRunImportTransactionFixture {
    func assertAmbiguousPreparationPreserved() throws {
        XCTAssertTrue(try entry.appendingPathComponent("vm.json").resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true)
        XCTAssertEqual(try Data(contentsOf: entry.appendingPathComponent("bundle/disks/hvf-target.raw")),
            try Data(contentsOf: disk))
        XCTAssertEqual(try Data(contentsOf: entry.appendingPathComponent("bundle/metadata/hvf-vars.fd")),
            try Data(contentsOf: vars))
    }
}

extension HVFCreationPublicationFixture {
    func assertAmbiguousPreparationPreserved() throws {
        let config = try XCTUnwrap(capturedConfig)
        XCTAssertTrue(try registration(config).resourceValues(forKeys: [.isDirectoryKey]).isDirectory == true)
        XCTAssertTrue(FileManager.default.fileExists(atPath: bundle(config).path))
        if config.installPending == true {
            let request = try XCTUnwrap(HvfWindowsInstallRequest.load(bundlePath: config.bundlePath))
            XCTAssertFalse(try Data(contentsOf: URL(fileURLWithPath: request.isoPath)).isEmpty)
        } else {
            XCTAssertTrue(FileManager.default.fileExists(atPath: try XCTUnwrap(config.diskPath)))
            XCTAssertTrue(FileManager.default.fileExists(atPath: bundle(config).appendingPathComponent("metadata/hvf-vars.fd").path))
        }
    }
}
