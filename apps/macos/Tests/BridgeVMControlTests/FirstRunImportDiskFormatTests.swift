import XCTest
@testable import BridgeVMControl

final class FirstRunImportDiskFormatTests: XCTestCase {
    func testQCOW2CannotBePublishedAsRaw() throws {
        try assertUnsupportedHeader(Data([0x51, 0x46, 0x49, 0xfb]), format: "QCOW2")
    }

    func testVHDXCannotBePublishedAsRaw() throws {
        try assertUnsupportedHeader(Data("vhdxfile".utf8), format: "VHDX")
    }

    func testRawContentWithContainerExtensionStillImports() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let disk = fixture.root.appendingPathComponent("installed.qcow2")
        try FileManager.default.moveItem(at: fixture.disk, to: disk)
        var inputs = fixture.inputs()
        inputs.diskPath = disk.path

        let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        guard case .committed(let config) = result else {
            return XCTFail("Raw content remains importable regardless of the filename extension")
        }
        XCTAssertEqual(VMLibrary.list(rootURL: fixture.library).map(\.id), [config.id])
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(config.diskPath))),
            try Data(contentsOf: disk))
        try FileManager.default.moveItem(at: disk, to: fixture.disk)
        try fixture.assertPersisted()
    }

    private func assertUnsupportedHeader(
        _ header: Data, format: String, file: StaticString = #filePath, line: UInt = #line
    ) throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        var sourceDisk = header
        sourceDisk.count = 4096
        try sourceDisk.write(to: fixture.disk)
        let sourceVars = try Data(contentsOf: fixture.vars)

        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        if case .failed(let message) = result {
            XCTAssertTrue(message.contains(format), message, file: file, line: line)
        } else {
            XCTFail("\(format) must be refused before a raw VM is published", file: file, line: line)
        }
        XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty, file: file, line: line)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path), file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: fixture.disk), sourceDisk, file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: fixture.vars), sourceVars, file: file, line: line)
    }
}
