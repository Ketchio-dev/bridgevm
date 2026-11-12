import Foundation
import XCTest
@testable import BridgeVMControl

/// The snapshot card asks before it restores or replaces the VM's one snapshot, runs nothing until
/// the user agrees, and says every failure in one Korean sentence (HvfWindowsSnapshotModel).
@MainActor
final class HvfWindowsSnapshotModelTests: XCTestCase {
    typealias Operation = HvfWindowsSnapshotCommand.Operation
    private let made = Date(timeIntervalSince1970: 1_794_000_000)

    func testRestoreRunsOnlyOnceConfirmedAndCancelRunsNothing() async throws {
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        let run: HvfWindowsSnapshotModel.Run = { ran.append($0) }

        XCTAssertNil(model.request(.restore, snapshotDate: made, run: run))
        let asked = try XCTUnwrap(model.confirmation)
        XCTAssertEqual(asked.title, "\(HvfWindowsSnapshotModel.dateText(made))의 스냅샷으로 되돌릴까요?")
        XCTAssertEqual(asked.message, "그 뒤에 바뀐 디스크 내용은 모두 사라집니다.")
        XCTAssertEqual(asked.action, "되돌리기")
        XCTAssertEqual(ran, [], "asking runs nothing")
        model.cancel()
        XCTAssertNil(model.confirmation)
        await Task.yield()
        XCTAssertEqual(ran, [], "cancel runs nothing")
        XCTAssertFalse(model.busy)
        XCTAssertEqual(model.status, HvfWindowsSnapshotModel.idleText)

        model.request(.restore, snapshotDate: made, run: run)
        await model.confirm(try XCTUnwrap(model.confirmation))?.value
        XCTAssertEqual(ran, [.restore])
        XCTAssertNil(model.confirmation)
        XCTAssertEqual(model.status, "스냅샷 복원 완료")
        XCTAssertFalse(model.failed)
    }

    func testCreateAsksOnlyWhenItWouldReplaceTheSnapshot() async throws {
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        let run: HvfWindowsSnapshotModel.Run = { ran.append($0) }

        XCTAssertNil(model.request(.create, snapshotDate: made, run: run))
        let asked = try XCTUnwrap(model.confirmation)
        XCTAssertEqual(asked.title, "\(HvfWindowsSnapshotModel.dateText(made))의 스냅샷을 새 스냅샷으로 바꿀까요?")
        XCTAssertEqual(asked.action, "바꾸기")
        model.cancel()
        await Task.yield()
        XCTAssertEqual(ran, [], "cancel keeps the old snapshot")

        await model.request(.create, snapshotDate: nil, run: run)?.value
        XCTAssertEqual(ran, [.create], "nothing to replace, nothing to ask")
        XCTAssertNil(model.confirmation)
        XCTAssertEqual(model.status, "스냅샷 생성 완료")
    }

    func testRestoreWithoutASnapshotRunsNothing() async {
        let model = HvfWindowsSnapshotModel()
        var ran: [Operation] = []
        XCTAssertNil(model.request(.restore, snapshotDate: nil) { ran.append($0) })
        await Task.yield()
        XCTAssertEqual(ran, [])
        XCTAssertNil(model.confirmation)
        XCTAssertEqual(model.status, "되돌릴 스냅샷이 없으니 먼저 스냅샷을 만드세요.")
        XCTAssertTrue(model.failed)
    }

    /// main showed "스냅샷 실패: The operation couldn't be completed. (BridgeVM.RuntimeMutation error 1.)".
    func testAFailedRunShowsItsSentenceNeverTheEmptyError() async {
        let model = HvfWindowsSnapshotModel()
        await model.request(.create, snapshotDate: nil) { _ in
            throw NSError(domain: "BridgeVM.RuntimeMutation", code: 1)
        }?.value
        XCTAssertEqual(model.status, "VM이 실행 중이거나 다른 작업 중이라 스냅샷을 만들지 못했으니 VM을 완전히 종료한 뒤 다시 시도하세요.")
        XCTAssertTrue(model.failed)
        XCTAssertFalse(model.busy)
    }

    /// Each failure the snapshot path reports, as it reaches the card, and the helper's own refusals.
    func testEachKnownFailureReadsAsOneKoreanSentence() throws {
        typealias Command = HvfWindowsSnapshotCommand
        let notStopped = "VM이 실행 중이거나 다른 작업 중이라 스냅샷을 만들지 못했으니 VM을 완전히 종료한 뒤 다시 시도하세요."
        let stale = HvfRuntimeMutationReservation(kind: .snapshot, configuration: HvfEngineView.defaultConfig())
        stale.invalidate()
        let cases: [(Error, Operation, String)] = [
            (NSError(domain: "BridgeVM.RuntimeMutation", code: 1), .create, notStopped),
            (Result { try stale.checkEffect() }.failure!, .create, notStopped),
            (Command.notStopped(), .restore,
             "VM이 실행 중이거나 다른 작업 중이라 스냅샷으로 되돌리지 못했으니 VM을 완전히 종료한 뒤 다시 시도하세요."),
            (Command.failure("settings edited on this page are not saved yet", .settingsUnsaved), .create,
             "이 화면에서 바꾼 설정이 아직 적용되지 않아 스냅샷을 만들지 못했으니 바꾼 내용을 되돌리거나 VM을 한 번 시작해 적용한 뒤 다시 시도하세요."),
            (try helper("media is in use by a running VM or another snapshot operation: held by pid 42"), .create, notStopped),
            (try helper("No space left on device (os error 28)"), .create,
             "Mac 저장 공간이 부족해 스냅샷을 만들지 못했으니 공간을 확보한 뒤 다시 시도하세요."),
            (try helper("No space left on device; free space on your Mac and try again."), .restore,
             "Mac 저장 공간이 부족해 스냅샷으로 되돌리지 못했으니 공간을 확보한 뒤 다시 시도하세요."),
            (try helper("disk.raw does not match the hash recorded in the manifest"), .restore,
             "스냅샷이 손상되어 되돌리지 못했으니 새 스냅샷을 만드세요."),
            (try helper("unusable snapshot manifest: unknown format_version 9"), .create,
             "새 스냅샷이 검증을 통과하지 못했으니 스냅샷을 다시 만드세요."),
            (try helper("Operation not permitted (os error 1)"), .create,
             "스냅샷을 만들지 못했으니 다시 시도하고, 계속 실패하면 아래 이벤트 피드에서 자세한 내용을 확인하세요."),
            (try helper(nil), .restore,
             "스냅샷으로 되돌리지 못했으니 다시 시도하고, 계속 실패하면 아래 이벤트 피드에서 자세한 내용을 확인하세요."),
            (Result { try HvfWindowsSnapshotQuota.parse("unexpected\n") }.failure!, .create,
             "스냅샷을 만들지 못했으니 다시 시도하고, 계속 실패하면 아래 이벤트 피드에서 자세한 내용을 확인하세요."),
        ]
        for (error, operation, expected) in cases {
            let message = HvfWindowsSnapshotModel.message(for: error, operation: operation)
            XCTAssertEqual(message, expected, error.localizedDescription)
            Self.assertPlainKoreanSentence(message)
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
    /// waits for its answer, and a damaged snapshot refused in Korean with the helper's words in the feed.
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
            try await session.runSnapshot($0, configuration: config)
        }

        XCTAssertNil(HvfWindowsSnapshotCommand.snapshotDate(config: config))
        await model.request(.create, snapshotDate: nil, run: run)?.value
        XCTAssertEqual(model.status, "스냅샷 생성 완료", session.events.last?.displayText ?? "")
        let date = try XCTUnwrap(HvfWindowsSnapshotCommand.snapshotDate(config: config))

        model.request(.restore, snapshotDate: date, run: run)
        XCTAssertEqual(ran, [.create], "the restore waits for its answer")
        await model.confirm(try XCTUnwrap(model.confirmation))?.value
        XCTAssertEqual(model.status, "스냅샷 복원 완료", session.events.last?.displayText ?? "")
        XCTAssertEqual(HvfWindowsSnapshotCommand.snapshotDate(config: config), date, "a restore keeps the snapshot")

        let snapshot = bundle.appendingPathComponent(HvfWindowsSnapshotCommand.slot)
        try Data(repeating: 1, count: 128).write(to: snapshot.appendingPathComponent("disk.raw"))
        model.request(.restore, snapshotDate: date, run: run)
        await model.confirm(try XCTUnwrap(model.confirmation))?.value
        XCTAssertEqual(ran, [.create, .restore, .restore])
        XCTAssertEqual(model.status, "스냅샷이 손상되어 되돌리지 못했으니 새 스냅샷을 만드세요.")
        XCTAssertTrue(model.failed)
        let feed = session.events.last?.displayText ?? ""
        XCTAssertTrue(feed.hasPrefix("powered-off snapshot restore failed (BridgeVM.HvfWindowsSnapshot 1): "), feed)
        XCTAssertTrue(feed.contains("does not match the hash"), feed)
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
            Self.assertPlainKoreanSentence(HvfWindowsSnapshotModel.message(for: error, operation: .create),
                                           file: file, line: line)
        }
        XCTAssertEqual(session.events.count, events + 1, "the detail goes to the feed", file: file, line: line)
        XCTAssertFalse(session.hasPendingRuntimeMutation, file: file, line: line)
    }

    /// One sentence in Korean: no helper English, no NSError text; only the product's own names.
    static func assertPlainKoreanSentence(_ message: String, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertTrue(message.hasSuffix("."), message, file: file, line: line)
        XCTAssertEqual(message.filter { $0 == "." }.count, 1, "one sentence: \(message)", file: file, line: line)
        XCTAssertTrue(message.unicodeScalars.contains { (0xAC00...0xD7A3).contains($0.value) }, message, file: file, line: line)
        var rest = message
        for name in ["BridgeVM", "VM", "Mac"] { rest = rest.replacingOccurrences(of: name, with: "") }
        XCTAssertFalse(rest.unicodeScalars.contains { CharacterSet.letters.contains($0) && $0.isASCII },
                       "no English: \(message)", file: file, line: line)
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
