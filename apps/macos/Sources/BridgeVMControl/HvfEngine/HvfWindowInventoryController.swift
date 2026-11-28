import Foundation
import Combine
import BridgeVMWindowProtocol

@MainActor
final class HvfWindowInventoryController: ObservableObject {
    @Published private(set) var records: [GuestWindowRecord] = []
    @Published private(set) var status = "Choose Refresh to list open windows."
    @Published private(set) var isLoading = false
    private weak var session: HvfEngineSession?
    private let logURL: URL
    private let reader: TailOffsetReader
    private var pending: HvfWindowInventoryRequest?
    private var timer: Timer?

    init(session: HvfEngineSession) {
        self.session = session
        logURL = URL(fileURLWithPath: session.config.evidenceDir).appendingPathComponent("run.log")
        let attrs = try? FileManager.default.attributesOfItem(atPath: logURL.path)
        reader = TailOffsetReader(startingAt: (attrs?[.size] as? NSNumber)?.uint64Value ?? 0)
    }

    deinit { timer?.invalidate() }

    func start() {
        guard timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
        refresh()
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        invalidate("Closed")
    }

    @discardableResult
    func refresh() -> Bool {
        poll()
        guard pending == nil, let session, case .connected = session.connectionState else { return false }
        let request = HvfWindowInventoryRequest(now: session.guestNow())
        guard session.sendCtl(request.command) else {
            invalidate("Guest Tools isn’t connected. Try again after Windows finishes starting.")
            return false
        }
        pending = request
        isLoading = true
        status = "Loading…"
        return true
    }

    func poll() {
        session?.poll()
        guard let session, case .connected = session.connectionState else {
            invalidate("Windows isn’t connected. Choose Refresh after Windows starts.")
            return
        }
        let lines = reader.readNewLines(from: logURL)
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY ") || $0.hasPrefix("BVAGENT re-READY ")
                || $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) {
            invalidate(Self.restarted)
            return
        }
        guard let result = pending?.consume(lines: lines, now: session.guestNow()) else { return }
        pending = nil
        isLoading = false
        switch result {
        case let .success(windows):
            records = windows
            status = windows.isEmpty ? "No windows are open in Windows."
                : windows.count == 1 ? "1 open window." : "\(windows.count) open windows."
        case let .failure(error):
            NSLog("Window list failed: %@", String(describing: error))
            invalidate(Self.message(error))
        }
    }

    static let restarted = "Windows restarted. Choose Refresh to reload the list."

    /// What the panel says for a failed list; the case name goes to the log only.
    static func message(_ error: HvfWindowInventoryError) -> String {
        switch error {
        case .restarted: return restarted
        case .resourceLimit: return "Windows has too many open windows to list."
        case .expired, .guestError, .malformedRecord, .duplicateHandle:
            return "Couldn’t get the window list from Windows. Choose Refresh to try again."
        }
    }

    private func invalidate(_ reason: String) {
        pending = nil
        isLoading = false
        records = []
        status = reason
    }
}
