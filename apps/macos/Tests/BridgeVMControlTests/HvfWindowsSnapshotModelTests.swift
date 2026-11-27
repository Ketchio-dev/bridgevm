import Foundation
import XCTest
@testable import BridgeVMControl

/// The snapshot card confirms destructive actions and reports actionable failures in English.
@MainActor
final class HvfWindowsSnapshotModelTests: XCTestCase {
    typealias Operation = HvfWindowsSnapshotCommand.Operation
    private let made = Date(timeIntervalSince1970: 1_794_000_000)

    func testRestoreRunsOnlyAfterTheDisplayedQuestionIsConfirmed() async throws {
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        let run: HvfWindowsSnapshotModel.Run = { ran.append($0); return nil }

        let asked = try XCTUnwrap(model.confirmation(for: .restore, snapshotDate: made, run: run))
        XCTAssertEqual(asked.title, "Restore “\(HvfWindowsSnapshotModel.dateText(made))”?")
        XCTAssertEqual(asked.message, "Restoring “\(HvfWindowsSnapshotModel.dateText(made))” discards all disk changes made since this snapshot.")
        XCTAssertEqual(asked.action, "Restore")
        XCTAssertEqual(ran, [], "asking runs nothing")
        await Task.yield()
        XCTAssertEqual(model.listRevision, 0)
        XCTAssertFalse(model.busy)
        XCTAssertNil(model.status)

        await model.confirm(asked)?.value
        XCTAssertEqual(ran, [.restore])
        XCTAssertEqual(model.listRevision, 1)
        XCTAssertEqual(model.status, "Snapshot restored")
        XCTAssertFalse(model.failed)
    }

    func testCreateNeverReplacesAndDeleteNamesItsConfirmedSnapshot() async throws {
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        let run: HvfWindowsSnapshotModel.Run = { ran.append($0); return nil }
        await model.create(run: run)?.value
        XCTAssertEqual(ran, [.create])
        let asked = try XCTUnwrap(model.confirmation(for: .delete, snapshotDate: made, label: "설치 완료", run: run))
        XCTAssertEqual(asked.title, "Delete “설치 완료”?")
        XCTAssertEqual(asked.message, "This permanently deletes the snapshot “설치 완료” and preserves the current VM disk.")
        XCTAssertEqual(ran, [.create])
        await model.confirm(asked)?.value
        XCTAssertEqual(ran, [.create, .delete])
        XCTAssertEqual(model.listRevision, 2)
        XCTAssertEqual(model.status, "Snapshot deleted")
    }

    func testRestoreWithoutASnapshotRunsNothing() async {
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        XCTAssertNil(model.confirmation(for: .restore, snapshotDate: nil) { ran.append($0); return nil })
        await Task.yield()
        XCTAssertEqual(ran, [])
        XCTAssertEqual(model.status, "Create a snapshot before trying to restore one.")
        XCTAssertTrue(model.failed)
    }

    /// main showed "스냅샷 실패: The operation couldn't be completed. (BridgeVM.RuntimeMutation error 1.)".
    func testAFailedRunShowsItsSentenceNeverTheEmptyError() async {
        let model = HvfWindowsSnapshotModel()
        await model.create { _ in
            throw NSError(domain: "BridgeVM.RuntimeMutation", code: 1)
        }?.value
        XCTAssertEqual(model.status, "Shut down the VM and wait for other operations to finish before creating a snapshot.")
        XCTAssertTrue(model.failed)
        XCTAssertFalse(model.busy)
    }

    /// Each failure the snapshot path reports, as it reaches the card, and the helper's own refusals.
    func testEachKnownFailureGivesOneActionableSentence() throws {
        typealias Command = HvfWindowsSnapshotCommand
        let notStopped = "Shut down the VM and wait for other operations to finish before creating a snapshot."
        let stale = HvfRuntimeMutationReservation(kind: .snapshot, configuration: HvfEngineView.defaultConfig())
        stale.invalidate()
        let cases: [(Error, Operation, String)] = [
            (NSError(domain: "BridgeVM.RuntimeMutation", code: 1), .create, notStopped),
            (Result { try stale.checkEffect() }.failure!, .create, notStopped),
            (Command.notStopped(), .restore,
             "Shut down the VM and wait for other operations to finish before restoring the snapshot."),
            (Command.failure("settings edited on this page are not saved yet", .settingsUnsaved), .create,
             "Undo the pending settings changes or start the VM once to apply them before creating a snapshot."),
            (try helper("media is in use by a running VM or another snapshot operation: held by pid 42"), .create, notStopped),
            (try helper("No space left on device (os error 28)"), .create,
             "Free up space on the drive that holds the VM before creating a snapshot."),
            (try helper("No space left on device; free space on your Mac and try again."), .restore,
             "Free up space on the drive that holds the VM before restoring the snapshot."),
            (try helper("disk.raw does not match the hash recorded in the manifest"), .restore,
             "This snapshot is damaged, so create a new snapshot."),
            (try helper("unusable snapshot manifest: unknown format_version 9"), .create,
             "The new snapshot failed verification, so create it again."),
            (try helper("Operation not permitted (os error 1)"), .create,
             "Try creating a snapshot again and check the event feed for details if the problem continues."),
            (try helper(nil), .restore,
             "Try restoring the snapshot again and check the event feed for details if the problem continues."),
        ]
        for (error, operation, expected) in cases {
            let message = HvfWindowsSnapshotModel.message(for: error, operation: operation)
            XCTAssertEqual(message, expected, error.localizedDescription)
            Self.assertPlainSentence(message)
        }
    }

    /// A refusal before any effect reaches the card as an error with text, and its detail goes to the
    /// event feed; the reservation is released.
    func testSessionRefusalsHaveTheirKindAndTheDetailGoesToTheFeed() async throws {
        let f = try HvfOwnedRuntimeFixture(); defer { f.clean() }
        let session = f.session()
        var edited = session.config
        edited.ramMiB += 128
        try await assertRefused(session, edited, .settingsUnsaved)
        session.reservedWorkAdmission = { _, _ in "The library registration was removed" }
        try await assertRefused(session, session.config, .notStopped)
        session.reservedWorkAdmission = nil
        try await assertRefused(session, session.config, .notLibraryVM) // the fixture's media has no bundle
        XCTAssertEqual(session.events.last?.displayText, "powered-off snapshot create failed (BridgeVM.HvfWindowsSnapshot "
            + "\(HvfWindowsSnapshotCommand.Failure.notLibraryVM.rawValue)): snapshot media is outside the managed Windows HVF bundle")
        XCTAssertFalse(session.hasActiveRuntimeWork)
    }

    /// The card's whole path with the shipped helper: a create with nothing to replace, a restore that
    /// waits for its answer, and a damaged snapshot refused with a recovery message with the helper's words in the feed.
    func testShippedHelperCreatesRestoresOnceConfirmedAndRefusesADamagedSnapshot() async throws {
        let root = FileManager.default.temporaryDirectory.resolvingSymlinksInPath()
            .appendingPathComponent("snapshot-model-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let library = root.appendingPathComponent("library"), repo = root.appendingPathComponent("repo")
        let bundle = library.appendingPathComponent("snapshot-vm/bundle.vmbridge")
        let disk = bundle.appendingPathComponent("disks/hvf-target.raw")
        let vars = bundle.appendingPathComponent("metadata/hvf-vars.fd")
        let helper = repo.appendingPathComponent("target/release/examples/snapshot_pair_cli")
        for directory in [disk, vars, helper].map({ $0.deletingLastPathComponent() }) {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        try Data(repeating: 7, count: 128).write(to: disk)
        try Data(repeating: 9, count: 16).write(to: vars)
        try FileManager.default.copyItem(at: HvfMediaImportTestSupport.helper, to: helper)
        let config = try XCTUnwrap(HvfEngineConfig.libraryVM(VMConfig(
            id: "snapshot-vm", name: "Snapshot VM", displayName: "Snapshot VM", backendKind: "hvf-engine",
            bootMode: "windows-hvf", bundlePath: bundle.path, runnerPath: "", launchSpecPath: "",
            handoffPath: "", sshKeyPath: "", sshUser: "", leasesPath: "", guestName: "Windows",
            displayWidth: 1280, displayHeight: 800, installPending: false, memMiB: 6144,
            cpuCount: 4, experimental3DAllowed: false), rootURL: library))
        let session = HvfEngineSession(config: config, repoRoot: repo) { _ in false }
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        let run: HvfWindowsSnapshotModel.Run = {
            ran.append($0)
            return try await session.runSnapshot($0, configuration: config)
        }

        XCTAssertNil(HvfWindowsSnapshotCommand.snapshotDate(config: config))
        await model.create(run: run)?.value
        XCTAssertEqual(model.status, "Snapshot created", session.events.last?.displayText ?? "")
        let date = try XCTUnwrap(HvfWindowsSnapshotCommand.snapshotDate(config: config))
        let first = try XCTUnwrap(HvfWindowsSnapshotStore.list(bundle: bundle).first)
        try Data(repeating: 5, count: 128).write(to: disk)
        try await session.runSnapshot(.create, configuration: config, name: "두 번째/../이름")
        let entries = HvfWindowsSnapshotStore.list(bundle: bundle)
        XCTAssertEqual(entries.count, 2)
        XCTAssertNotEqual(entries[0].url, entries[1].url)
        XCTAssertEqual(entries.first?.name, "두 번째/../이름")
        let restoreFirst: HvfWindowsSnapshotModel.Run = {
            ran.append($0)
            return try await session.runSnapshot($0, configuration: config, target: first.url)
        }
        let restore = try XCTUnwrap(model.confirmation(for: .restore, snapshotDate: date, label: first.label, run: restoreFirst))
        XCTAssertEqual(ran, [.create], "the restore waits for its answer")
        await model.confirm(restore)?.value
        XCTAssertEqual(model.status, "Snapshot restored", session.events.last?.displayText ?? "")
        let selectedDigest = try HvfWindowsSnapshotCommand.invoke(helper, ["digest", disk.path, vars.path])
        let firstDigest = try HvfWindowsSnapshotCommand.invoke(helper, ["digest", first.url.appendingPathComponent("disk.raw").path,
                                                                        first.url.appendingPathComponent("vars.fd").path])
        XCTAssertEqual(selectedDigest, firstDigest, "restore selects the first pair without overwriting logical originals")
        XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle).count, 2, "a restore keeps both snapshots")

        let snapshot = first.url
        try Data(repeating: 1, count: 128).write(to: snapshot.appendingPathComponent("disk.raw"))
        let corruptRestore = try XCTUnwrap(model.confirmation(for: .restore, snapshotDate: date, run: restoreFirst))
        await model.confirm(corruptRestore)?.value
        XCTAssertEqual(ran, [.create, .restore, .restore])
        XCTAssertEqual(model.status, "This snapshot is damaged, so create a new snapshot.")
        XCTAssertTrue(model.failed)
        let feed = session.events.last?.displayText ?? ""
        XCTAssertTrue(feed.hasPrefix("powered-off snapshot restore failed (BridgeVM.HvfWindowsSnapshot 1): "), feed)
        XCTAssertTrue(feed.contains("does not match the hash"), feed)
        let delete = try XCTUnwrap(model.confirmation(for: .delete, snapshotDate: date, label: first.label, run: restoreFirst))
        await model.confirm(delete)?.value
        XCTAssertEqual(model.status, "Snapshot deleted")
        XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle).count, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: first.url.path))
        XCTAssertEqual(try HvfWindowsSnapshotCommand.invoke(helper, ["digest", disk.path, vars.path]), selectedDigest)
        // A list shown before that delete still offers the snapshot.
        let staleRestore = try XCTUnwrap(model.confirmation(for: .restore, snapshotDate: date, label: first.label, run: restoreFirst))
        await model.confirm(staleRestore)?.value
        XCTAssertEqual(model.status, "This snapshot no longer exists, so choose another one.")
        await model.confirm(delete)?.value
        XCTAssertEqual(model.status, "This snapshot was already deleted.")

        // The helper owns staging; a failed invocation must not publish an entry.
        let before = HvfWindowsSnapshotStore.list(bundle: bundle)
        try Data("#!/bin/sh\nexit 1\n".utf8).write(to: helper)
        await model.create(run: run)?.value
        XCTAssertTrue(model.failed)
        XCTAssertEqual(HvfWindowsSnapshotStore.list(bundle: bundle), before)
        XCTAssertFalse(session.hasActiveRuntimeWork)
    }

    private func assertRefused(_ session: HvfEngineSession, _ configuration: HvfEngineConfig,
                               _ failure: HvfWindowsSnapshotCommand.Failure,
                               file: StaticString = #filePath, line: UInt = #line) async throws {
        let events = session.events.count
        do {
            try await session.runSnapshot(.create, configuration: configuration)
            XCTFail("refused", file: file, line: line)
        } catch {
            let error = error as NSError
            XCTAssertEqual(error.domain, HvfWindowsSnapshotCommand.errorDomain, file: file, line: line)
            XCTAssertEqual(error.code, failure.rawValue, file: file, line: line)
            XCTAssertFalse(error.localizedDescription.isEmpty, file: file, line: line)
            Self.assertPlainSentence(HvfWindowsSnapshotModel.message(for: error, operation: .create),
                                           file: file, line: line)
        }
        XCTAssertEqual(session.events.count, events + 1, "the detail goes to the feed", file: file, line: line)
        XCTAssertFalse(session.hasPendingRuntimeMutation, file: file, line: line)
    }

    /// The card gives a single recovery sentence and keeps internal error domains in the event feed.
    static func assertPlainSentence(_ message: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(message.hasSuffix("."), message, file: file, line: line)
        XCTAssertEqual(message.filter { $0 == "." }.count, 1, "one sentence: \(message)", file: file, line: line)
        XCTAssertFalse(message.contains("RuntimeMutation"), message, file: file, line: line)
    }

    /// What `snapshot_pair_cli` throws through `invoke` when it writes `stderr` (nothing for nil) and exits 1.
    private func helper(_ stderr: String?) throws -> Error {
        let script = stderr.map { "printf '%s\\n' '\($0)' >&2; exit 1" } ?? "exit 1"
        do {
            _ = try HvfWindowsSnapshotCommand.invoke(URL(fileURLWithPath: "/bin/sh"), ["-c", script])
        } catch { return error }
        throw XCTSkip("the helper stand-in succeeded")
    }
}

private extension Result {
    var failure: Failure? {
        if case let .failure(error) = self { return error }
        return nil
    }
}
