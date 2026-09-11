import Foundation
import Combine
import BridgeVMWindowProtocol

@MainActor
final class HvfWindowInventoryController: ObservableObject {
    @Published private(set) var records: [GuestWindowRecord] = []
    @Published private(set) var status = "Not queried"
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
    func refresh(now: Date = Date()) -> Bool {
        poll(now: now)
        guard pending == nil, let session, case .connected = session.connectionState else { return false }
        let request = HvfWindowInventoryRequest(now: now)
        guard session.sendCtl(request.command) else { invalidate("Guest service unavailable"); return false }
        pending = request
        isLoading = true
        status = "Loading"
        return true
    }

    func poll(now: Date = Date()) {
        session?.poll()
        guard let session, case .connected = session.connectionState else {
            invalidate("Disconnected")
            return
        }
        let lines = reader.readNewLines(from: logURL)
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY ") || $0.hasPrefix("BVAGENT re-READY ")
                || $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI SYSTEM_RESET:")
        }) {
            invalidate("Guest restarted")
            return
        }
        guard let result = pending?.consume(lines: lines, now: now) else { return }
        pending = nil
        isLoading = false
        switch result {
        case let .success(windows):
            records = windows
            status = windows.isEmpty ? "No guest windows" : "\(windows.count) guest windows"
        case let .failure(error):
            invalidate("Query failed: \(error)")
        }
    }

    private func invalidate(_ reason: String) {
        pending = nil
        isLoading = false
        records = []
        status = reason
    }
}
