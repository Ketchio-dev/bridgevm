import Foundation
import XCTest
@testable import BridgeVMControl

/// The library deletion path retires an HVF VM's vTPM key only after its managed
/// bundle was really removed. Deletion runs the real HVF protected deletion with
/// the fixture's scripted media-lease helper; keys live in an in-memory store.
final class LibraryDeletionKeyArchiveTests: XCTestCase {
    private let key = Data(repeating: 0x11, count: KeychainVTPMStateKeyStore.keyLength)

    private func delete(_ config: VMConfig, in f: HvfPendingDeletionFixture, store: VTPMDeletionTestKeyStore,
                        running: Bool = false) -> LibraryDeletionResult {
        LibraryDeletionKeyArchive.perform(config, rootURL: f.library, isRunning: { running }, keyStore: store,
                                          delete: f.scriptedDelete)
    }

    func testManagedDeletionArchivesTheKeyAndRemovesTheActiveSlugKey() throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try f.installedManaged()
        XCTAssertEqual(VMLibrary.deletionImpact(for: installed, rootURL: f.library), .managedBundleDeleted)
        let store = VTPMDeletionTestKeyStore()
        store.keys[installed.slug] = key
        XCTAssertEqual(delete(installed, in: f, store: store), LibraryDeletionResult(outcome: .deleted))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertNil(store.keys[installed.slug])
        let archived = store.archivedKeyIDs(of: installed.slug)
        XCTAssertEqual(archived.count, 1)
        XCTAssertEqual(archived.first.flatMap { store.keys[$0] }, key)
        let suffix = try XCTUnwrap(archived.first?.dropFirst(installed.slug.count + ".deleted.".count))
        let parts = suffix.split(separator: "-")
        XCTAssertEqual(parts.count, 2, String(suffix))
        XCTAssertNotNil(parts.first.flatMap { Int($0) }, String(suffix))
        XCTAssertEqual(parts.last?.count, 8, String(suffix))
    }

    func testPendingManagedDeletionAlsoArchivesAnExistingKey() throws {
        let f = try HvfPendingDeletionFixture()
        let store = VTPMDeletionTestKeyStore()
        store.keys[f.config.slug] = key
        XCTAssertEqual(delete(f.config, in: f, store: store).outcome, .deleted)
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertNil(store.keys[f.config.slug])
        XCTAssertEqual(store.archivedKeyIDs(of: f.config.slug).count, 1)
    }

    func testRegistrationOnlyRemovalKeepsTheKeyWithoutConsultingTheStore() throws {
        let f = try HvfPendingDeletionFixture(external: true)
        XCTAssertEqual(VMLibrary.deletionImpact(for: f.config, rootURL: f.library), .registrationOnly)
        let store = VTPMDeletionTestKeyStore()
        store.keys[f.config.slug] = key
        XCTAssertEqual(delete(f.config, in: f, store: store), LibraryDeletionResult(outcome: .deleted))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertTrue(f.fm.fileExists(atPath: f.bundle.path))
        XCTAssertEqual(store.keys, [f.config.slug: key])
        XCTAssertEqual(store.calls, [])
    }

    func testManagedDeletionWithoutAKeyIsANoOp() throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try f.installedManaged()
        let store = VTPMDeletionTestKeyStore()
        XCTAssertEqual(delete(installed, in: f, store: store), LibraryDeletionResult(outcome: .deleted))
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(store.keys, [:])
        XCTAssertEqual(store.calls, ["read \(installed.slug)"])
    }

    func testArchiveFailureKeepsTheActiveKeyAndStillReportsTheDeletionWithAWarning() throws {
        let f = try HvfPendingDeletionFixture()
        let installed = try f.installedManaged()
        let store = VTPMDeletionTestKeyStore()
        store.keys[installed.slug] = key
        store.failsReplace = { $0.contains(".deleted.") }
        let result = delete(installed, in: f, store: store)
        XCTAssertEqual(result.outcome, .deleted)
        let warning = try XCTUnwrap(result.warning)
        XCTAssertTrue(warning.contains("vTPM 키를 보관하지 못했습니다"), warning)
        XCTAssertTrue(warning.contains("'\(installed.slug)'"), warning)
        XCTAssertFalse(f.fm.fileExists(atPath: f.entry.path))
        XCTAssertEqual(store.keys, [installed.slug: key])
        XCTAssertFalse(store.calls.contains("delete \(installed.slug)"))
    }

    func testRefusedRunningOrNonHVFDeletionNeverConsultsTheStore() throws {
        let refused = try HvfPendingDeletionFixture()
        let refusedConfig = try refused.installedManaged()
        let running = try HvfPendingDeletionFixture()
        let runningConfig = try running.installedManaged()
        let fastFixture = try HvfPendingDeletionFixture()
        var fast = fastFixture.config
        fast.backendKind = "fast-vz"
        try fastFixture.save(fast)
        XCTAssertEqual(VMLibrary.deletionImpact(for: fast, rootURL: fastFixture.library), .managedBundleDeleted)
        let store = VTPMDeletionTestKeyStore()
        store.keys[refusedConfig.slug] = key // Every fixture VM is named "Pending".
        let refusal = LibraryDeletionKeyArchive.perform(refusedConfig, rootURL: refused.library, isRunning: { false },
            keyStore: store, delete: { _, _ in (try? refused.delete(refusedConfig, refuse: true)) == true })
        XCTAssertEqual(refusal, LibraryDeletionResult(outcome: .failed))
        XCTAssertTrue(refused.fm.fileExists(atPath: refused.entry.path))
        XCTAssertEqual(delete(runningConfig, in: running, store: store, running: true),
                       LibraryDeletionResult(outcome: .running))
        XCTAssertTrue(running.fm.fileExists(atPath: running.entry.path))
        XCTAssertEqual(LibraryDeletionKeyArchive.perform(fast, rootURL: fastFixture.library, isRunning: { false },
            keyStore: store), LibraryDeletionResult(outcome: .deleted))
        XCTAssertFalse(fastFixture.fm.fileExists(atPath: fastFixture.entry.path))
        XCTAssertEqual(store.calls, [])
        XCTAssertEqual(store.keys, [refusedConfig.slug: key])
    }
}
