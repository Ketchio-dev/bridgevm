import Foundation
import XCTest
@testable import BridgeVMControl

final class VMRegistrationCreationTests: XCTestCase {
    private final class Results: @unchecked Sendable {
        private let lock = NSLock()
        private var values: [Bool] = []
        func append(_ value: Bool) { lock.lock(); defer { lock.unlock() }; values.append(value) }
        var committed: Int { lock.lock(); defer { lock.unlock() }; return values.filter { $0 }.count }
    }

    func testExclusivePublicationHasOneWinnerAndMutableCommitStillReplaces() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        let url = fixture.root.appendingPathComponent("vm.json")
        let results = Results()
        let inputs = [Data("first".utf8), Data("second".utf8)]
        DispatchQueue.concurrentPerform(iterations: 2) { index in
            results.append(VMRegistrationWriter.commit(inputs[index], to: url, replaceExisting: false).isCommitted)
        }
        XCTAssertEqual(results.committed, 1)
        XCTAssertTrue(inputs.contains(try Data(contentsOf: url)))
        let replacement = Data("mutable update".utf8)
        XCTAssertTrue(VMRegistrationWriter.commit(replacement, to: url).isCommitted)
        XCTAssertEqual(try Data(contentsOf: url), replacement)
        XCTAssertFalse(try FileManager.default.contentsOfDirectory(atPath: fixture.root.path)
            .contains { $0.hasPrefix(".vm-config-") })
    }

    func testExclusivePublicationRetainsBytesAfterSyncFailureAndDoesNotSyncCollision() throws {
        let fixture = try VMCreationRegistrationCollisionTests.Fixture()
        defer { fixture.remove() }
        let url = fixture.root.appendingPathComponent("vm.json")
        let winner = Data("published".utf8)
        let outcome = VMRegistrationWriter.commit(winner, to: url, replaceExisting: false, syncParent: { _ in
            throw CocoaError(.fileWriteUnknown)
        })
        guard case .publishedButUnsynced = outcome else { return XCTFail("published bytes need preservation") }
        let collision = VMRegistrationWriter.commit(Data("loser".utf8), to: url,
            replaceExisting: false, syncParent: { _ in XCTFail("refused publication must not sync") })
        guard case .notPublished = collision else { return XCTFail("existing registration must refuse") }
        XCTAssertEqual(try Data(contentsOf: url), winner)
    }
}
