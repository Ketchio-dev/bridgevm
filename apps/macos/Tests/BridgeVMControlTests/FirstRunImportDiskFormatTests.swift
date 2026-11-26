import XCTest
@testable import BridgeVMControl

final class FirstRunImportDiskFormatTests: XCTestCase {
    private final class Stages: @unchecked Sendable {
        private let lock = NSLock()
        private var values: [FirstRunImportStage] = []
        func append(_ stage: FirstRunImportStage) { lock.lock(); values.append(stage); lock.unlock() }
        var snapshot: [FirstRunImportStage] { lock.lock(); defer { lock.unlock() }; return values }
    }

    func testQCOW2IsConvertedToTheRawDiskItDescribes() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let (image, raw) = HvfQCOW2TestImage.make()
        try image.write(to: fixture.disk)
        // Some VM apps keep efi_vars.fd beside the disk as a link; a linked vars path must still give an independent copy.
        let linkedVars = fixture.root.appendingPathComponent("efi_vars.fd")
        try FileManager.default.createSymbolicLink(at: linkedVars, withDestinationURL: fixture.vars)
        let sourceMode = try FileManager.default.attributesOfItem(atPath: fixture.vars.path)[.posixPermissions] as? NSNumber
        var inputs = fixture.inputs()
        inputs.varsPath = linkedVars.path
        let stages = Stages()

        let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper, progress: { stages.append($0) })

        guard case .committed(let config) = result else { return XCTFail("a QCOW2 disk must import: \(result)") }
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: XCTUnwrap(config.diskPath))), raw)
        let layout = FirstRunImport.BundleLayout(bundleURL: URL(fileURLWithPath: config.bundlePath))
        XCTAssertEqual(try Data(contentsOf: layout.varsURL), try Data(contentsOf: fixture.vars))
        XCTAssertEqual(try FileManager.default.attributesOfItem(atPath: layout.varsURL.path)[.type] as? FileAttributeType,
                       .typeRegular)
        XCTAssertEqual(try FileManager.default.attributesOfItem(atPath: fixture.vars.path)[.posixPermissions] as? NSNumber,
                       sourceMode)
        let mode = try FileManager.default.attributesOfItem(atPath: XCTUnwrap(config.diskPath))[.posixPermissions]
        XCTAssertEqual((mode as? NSNumber)?.intValue, 0o600)
        XCTAssertEqual(try Data(contentsOf: fixture.disk), image, "the source is not modified")
        let seen = stages.snapshot
        XCTAssertEqual(seen.first, .validating)
        XCTAssertEqual(seen.dropFirst().first, .converting(.qcow2, percent: 0))
        XCTAssertTrue(seen.contains(.converting(.qcow2, percent: 100)), "\(seen)")
        XCTAssertEqual(seen.last, .saving)
        XCTAssertEqual(FirstRunImportStage.converting(.qcow2, percent: 0).label, "QCOW2 디스크를 변환하는 중…")
        XCTAssertEqual(FirstRunImportStage.converting(.vhdx, percent: 42).label, "VHDX 디스크를 변환하는 중… 42%")
    }

    func testClassicVHDFooterIsRecognizedIndependentlyOfPathExtension() throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        let disk = fixture.root.appendingPathComponent("installed.vhd")
        let raw = Data(repeating: 0x42, count: 4096)
        try raw.write(to: disk)
        XCTAssertNil(HvfImportedDiskFormat.detect(path: disk.path))

        var footer = Data(count: 512)
        footer.replaceSubrange(0..<8, with: "conectix".utf8)
        footer.replaceSubrange(12..<16, with: [0, 1, 0, 0])
        footer[63] = 2
        try (raw + footer).write(to: fixture.disk)
        XCTAssertEqual(HvfImportedDiskFormat.detect(path: fixture.disk.path), .vhd)
    }

    func testABackingFileIsRefusedWithTheConvertersSentence() throws {
        try assertRefused(HvfQCOW2TestImage.make(backingFile: true).image,
                          sentence: "이 디스크는 다른 디스크를 기반으로 하니 원래 VM 앱에서 하나의 디스크로 합친 뒤 다시 가져오세요.")
    }

    func testDamagedQCOW2AndVHDXHeadersAreRefusedByTheConverter() throws {
        for (header, sentence) in [(Data([0x51, 0x46, 0x49, 0xfb]), "지원하지 않는 디스크 형식이므로 원래 VM 앱에서 디스크를 변환한 뒤 다시 가져오세요."),
                                   (Data("vhdxfile".utf8), "디스크 이미지가 손상되었거나 구조가 올바르지 않으니")] {
            var sourceDisk = header
            sourceDisk.count = 4096
            try assertRefused(sourceDisk, sentence: sentence)
        }
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

    func testVDIAndSparseVMDKAreRefusedByBothWindowsImportForms() throws {
        let qemuImg = (ProcessInfo.processInfo.environment["PATH"] ?? "").split(separator: ":")
            .map { URL(fileURLWithPath: String($0)).appendingPathComponent("qemu-img") }
            .first { FileManager.default.isExecutableFile(atPath: $0.path) }
        guard let qemuImg else { throw XCTSkip("qemu-img creates the two ordinary test fixtures") }
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        try Data(repeating: 0x42, count: 1_048_576).write(to: fixture.disk)
        let image = fixture.root.appendingPathComponent("export.raw")
        for format in ["vdi", "vmdk"] {
            let converter = Process()
            converter.executableURL = qemuImg
            converter.arguments = ["convert", "-f", "raw", "-O", format, fixture.disk.path, image.path]
            try converter.run()
            converter.waitUntilExit()
            XCTAssertEqual(converter.terminationStatus, 0)
            var inputs = fixture.inputs()
            inputs.diskPath = image.path
            let result = FirstRunImportWorker.run(inputs, libraryRoot: fixture.library,
                                                 snapshotHelper: HvfMediaImportTestSupport.helper)
            guard case let .failed(message) = result else { return XCTFail("registered a \(format) container as RAW") }
            XCTAssertTrue(message.contains(format.uppercased()))
            XCTAssertTrue(message.contains("RAW, QCOW2 또는 VHDX"))
            XCTAssertEqual(VMLibrary.windowsHVFImportError(targetDiskPath: image.path, varsPath: fixture.vars.path), message)
            XCTAssertNil(VMLibrary.createWindowsHVF(name: "imported", targetDiskPath: image.path,
                varsPath: fixture.vars.path, storageDir: fixture.root, libraryRoot: fixture.library,
                snapshotHelper: HvfMediaImportTestSupport.helper))
            XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty)
            try FileManager.default.removeItem(at: image)
        }
    }

    private func assertRefused(
        _ sourceDisk: Data, sentence: String, file: StaticString = #filePath, line: UInt = #line
    ) throws {
        let fixture = try FirstRunImportTransactionFixture()
        defer { fixture.remove() }
        try sourceDisk.write(to: fixture.disk)
        let sourceVars = try Data(contentsOf: fixture.vars)

        let result = FirstRunImportWorker.run(fixture.inputs(), libraryRoot: fixture.library,
            snapshotHelper: HvfMediaImportTestSupport.helper)
        if case .failed(let message) = result {
            XCTAssertTrue(message.contains(sentence), message, file: file, line: line)
            XCTAssertFalse(message.contains("progress"), message, file: file, line: line)
        } else {
            XCTFail("the converter's refusal must stop the import", file: file, line: line)
        }
        XCTAssertTrue(VMLibrary.list(rootURL: fixture.library).isEmpty, file: file, line: line)
        XCTAssertFalse(FileManager.default.fileExists(atPath: fixture.entry.path), file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: fixture.disk), sourceDisk, file: file, line: line)
        XCTAssertEqual(try Data(contentsOf: fixture.vars), sourceVars, file: file, line: line)
    }
}
