import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeModelCacheTests: XCTestCase {
    func testAdmittedModelFactoryRunsOnceAndOnlyAfterOwnershipValidation() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("status-admission-\(UUID().uuidString)")
        let library = try NativeRuntimeLibraryHandle.open(rootURL: root, create: true)
        let owner = try NativeRuntimeOwner(library: library)
        defer {
            owner.close()
            try? FileManager.default.removeItem(atPath: owner.endpoint.directoryPath)
            try? FileManager.default.removeItem(at: root)
        }
        let cache = NativeRuntimeModelCache<Int>(owner: owner)
        var effects = 0
        XCTAssertNil(cache.retainedValue)
        XCTAssertEqual(try cache.model { effects += 1; return effects }, 1)
        XCTAssertEqual(try cache.model { effects += 1; return effects }, 1)
        XCTAssertEqual(effects, 1)
    }

    func testReplacedLibraryRefusesModelFactoryBeforeAnyEffects() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("status-admission-\(UUID().uuidString)")
        let path = root.appendingPathComponent("library")
        let library = try NativeRuntimeLibraryHandle.open(rootURL: path, create: true)
        let owner = try NativeRuntimeOwner(library: library)
        defer {
            owner.close()
            try? FileManager.default.removeItem(atPath: owner.endpoint.directoryPath)
            try? FileManager.default.removeItem(at: root)
        }
        let cache = NativeRuntimeModelCache<Int>(owner: owner)
        try FileManager.default.moveItem(at: path, to: root.appendingPathComponent("old-library"))
        try FileManager.default.createDirectory(at: path, withIntermediateDirectories: true)
        var effects = 0
        XCTAssertThrowsError(try cache.model { effects += 1; return effects })
        XCTAssertNil(cache.retainedValue)
        XCTAssertEqual(effects, 0)
    }
}
