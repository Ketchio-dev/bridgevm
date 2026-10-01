import Darwin
import XCTest
@testable import BridgeVMProductE2E

final class T17EvidenceDigestMemoryTests: XCTestCase {
    /// T17 hashes 64 GiB disks. Unpooled reads kept every chunk resident until the hash
    /// returned; in r53 the helper vanished while hashing two such disks back to back.
    func testDigestOfALargeSparseFileUsesBoundedMemory() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("t17-digest-\(UUID().uuidString).raw")
        XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: nil))
        defer { try? FileManager.default.removeItem(at: url) }
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: 1 << 30)
        try handle.close()
        let before = Self.peakResidentBytes()
        XCTAssertEqual(try T17Evidence.sha256(url), "49bc20df15e412a64472421e13fe86ff1c5165e18b2afccf160d4dc19fe68a14")
        XCTAssertLessThan(Self.peakResidentBytes() - before, 256 << 20, "a 1 GiB digest must not keep the file resident")
    }

    private static func peakResidentBytes() -> Int {
        var usage = rusage()
        getrusage(RUSAGE_SELF, &usage)
        return usage.ru_maxrss  // bytes on Darwin
    }
}
