import Foundation

/// Restore and delete capture the selected snapshot in their confirmation; create never replaces one.
/// Failures read as one plain sentence, with technical detail in the session event feed.
@MainActor
final class HvfWindowsSnapshotModel: ObservableObject {
    typealias Operation = HvfWindowsSnapshotCommand.Operation
    typealias Run = @MainActor (Operation) async throws -> String?

    /// The question before an operation that throws away the snapshot or the changes since it.
    struct Confirmation {
        let operation: Operation
        let label: String
        let run: Run

        var title: String {
            operation == .restore ? "Restore “\(label)”?" : "Delete “\(label)”?"
        }
        var message: String {
            operation == .restore ? "Restoring “\(label)” discards all disk changes made since this snapshot." : "This permanently deletes the snapshot “\(label)” and preserves the current VM disk."
        }
        var action: String { operation == .restore ? "Restore" : "Delete" }
    }

    @Published private(set) var busy = false
    @Published private(set) var status: String?
    @Published private(set) var failed = false
    @Published private(set) var listRevision = 0

    func requestListRefresh() { listRevision += 1 }

    @discardableResult
    func create(run: @escaping Run) -> Task<Void, Never>? {
        start(.create, run)
    }

    func confirmation(for operation: Operation, snapshotDate: Date?, label: String? = nil, run: @escaping Run) -> Confirmation? {
        guard !busy else { return nil }
        guard let snapshotDate else {
            status = Self.noSnapshot; failed = true
            return nil
        }
        return Confirmation(operation: operation, label: label ?? Self.dateText(snapshotDate), run: run)
    }

    func clearResult() {
        guard !busy else { return }
        status = nil; failed = false
    }

    /// The user agreed to `confirmation`, the one the dialog showed.
    @discardableResult
    func confirm(_ confirmation: Confirmation) -> Task<Void, Never>? {
        start(confirmation.operation, confirmation.run)
    }

    private func start(_ operation: Operation, _ run: @escaping Run) -> Task<Void, Never>? {
        guard !busy else { return nil }
        busy = true; failed = false
        status = operation == .create ? "Creating snapshot…" : operation == .restore ? "Restoring snapshot…" : "Deleting snapshot…"
        return Task {
            do {
                let notice = try await run(operation)
                status = notice ?? (operation == .create ? "Snapshot created" : operation == .restore ? "Snapshot restored" : "Snapshot deleted")
            } catch {
                status = Self.message(for: error, operation: operation); failed = true
            }
            busy = false
            requestListRefresh()
        }
    }

    /// Use the Mac’s regional date and time preferences, including seconds for nearby snapshots.
    nonisolated static func dateText(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateStyle = .long
        formatter.timeStyle = .medium
        return formatter.string(from: date)
    }

    nonisolated static let noSnapshot = "Create a snapshot before trying to restore one."

    /// One plain sentence: what happened and what to do. The helper's English (the Rust
    /// SnapshotError and io::Error texts) is matched for the failures that happen in practice;
    /// anything else gets the operation's general sentence.
    nonisolated static func message(for error: Error, operation: Operation) -> String {
        let error = error as NSError
        let action = operation == .create ? "creating a snapshot" : operation == .restore ? "restoring the snapshot" : "deleting the snapshot"
        let text = error.localizedDescription
        var failure: HvfWindowsSnapshotCommand.Failure?
        switch error.domain {
        case HvfWindowsSnapshotCommand.errorDomain: failure = .init(rawValue: error.code)
        case "BridgeVM.RuntimeMutation": failure = .notStopped
        default: break
        }
        if text.contains("media is in use") { failure = .notStopped } // the helper's writer-lease refusal
        switch failure {
        case .notStopped?:
            return "Shut down the VM and wait for other operations to finish before \(action)."
        case .settingsUnsaved?:
            return "Undo the pending settings changes or start the VM once to apply them before \(action)."
        case .notLibraryVM?:
            return "Open the VM from the BridgeVM library to use snapshots."
        case .helperMissing?:
            return "Reinstall BridgeVM to restore a missing app component, then try \(action) again."
        case .unsafeFolder?:
            return "This VM’s “snapshots” folder, or a snapshot in it, was replaced by a file or link. Restore it as a normal folder, then try \(action) again."
        case .noSnapshot?:
            return noSnapshot
        case .snapshotGone?:
            return operation == .delete
                ? "This snapshot was already deleted."
                : "This snapshot no longer exists, so choose another one."
        case .helper?, nil:
            break
        }
        if text.contains("No space left on device")
            || (error.domain == NSCocoaErrorDomain && error.code == NSFileWriteOutOfSpaceError) {
            return "Free up space on the drive that holds the VM before \(action)."
        }
        if text.contains("does not match the hash") || text.contains("unusable snapshot manifest") {
            return operation == .create
                ? "The new snapshot failed verification, so create it again."
                : "This snapshot is damaged, so create a new snapshot."
        }
        return "Try \(action) again."
    }
}
