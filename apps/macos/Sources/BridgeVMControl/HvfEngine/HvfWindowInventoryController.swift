import Foundation
import Combine
import BridgeVMWindowProtocol

@MainActor
final class HvfWindowInventoryController: ObservableObject {
    @Published private(set) var records: [GuestWindowRecord] = []
    @Published private(set) var status = "아직 조회하지 않았습니다"
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
        guard session.sendCtl(request.command) else { invalidate("Windows 게스트 서비스에 연결되지 않았습니다"); return false }
        pending = request
        isLoading = true
        status = "불러오는 중…"
        return true
    }

    func poll() {
        session?.poll()
        guard let session, case .connected = session.connectionState else {
            invalidate("연결되지 않음")
            return
        }
        let lines = reader.readNewLines(from: logURL)
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY ") || $0.hasPrefix("BVAGENT re-READY ")
                || $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) {
            invalidate("Windows가 다시 시작되었습니다. 새로 고침을 누르세요.")
            return
        }
        guard let result = pending?.consume(lines: lines, now: session.guestNow()) else { return }
        pending = nil
        isLoading = false
        switch result {
        case let .success(windows):
            records = windows
            status = windows.isEmpty ? "열린 Windows 창이 없습니다" : "Windows 창 \(windows.count)개"
        case let .failure(error):
            invalidate("조회하지 못했습니다: \(error)")
        }
    }

    private func invalidate(_ reason: String) {
        pending = nil
        isLoading = false
        records = []
        status = reason
    }
}
