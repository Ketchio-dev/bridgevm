import Foundation
import Combine

/// One VM window's Windows apps: the list the guest last gave, asked again when the menu bar opens and the list is
/// older than `staleAfter` (or on 목록 새로 고침), and the launches waiting for the guest's answer. run.log is read
/// only while something waits, from where it ended when the question went out.
@MainActor
final class HvfGuestStartAppsController: ObservableObject {
    enum Status: Equatable { case idle, loading, loaded, failed(String) }
    static let staleAfter: TimeInterval = 30
    @Published private(set) var apps: [HvfGuestStartApp] = []
    @Published private(set) var status = Status.idle
    /// A launch the guest refused, in one sentence (the VM window shows it).
    var onNotice: (@MainActor (String) -> Void)?
    private weak var session: HvfEngineSession?
    private var pending: HvfGuestStartAppsRequest?
    private var launches: [HvfGuestStartAppLaunch] = []
    private var reader: TailOffsetReader?
    private var timer: Timer?
    private var loadedAt: Date?

    init(session: HvfEngineSession) { self.session = session }

    deinit { timer?.invalidate() }

    var unavailableReason: String? { HvfGuestStartApps.unavailableReason(session) }
    var menuApps: [HvfGuestStartApp] { HvfGuestStartApps.menuApps(apps) }

    /// What the menu says while it lists no app.
    var statusText: String {
        switch status {
        case .idle: return HvfGuestStartApps.notLoaded
        case .loading: return HvfGuestStartApps.loading
        case .loaded: return HvfGuestStartApps.empty
        case let .failed(message): return message
        }
    }

    /// Asks again unless a question is under way or the list is younger than `staleAfter`.
    func loadIfStale(now: Date = Date()) {
        if status == .loaded, let loadedAt, now.timeIntervalSince(loadedAt) < Self.staleAfter { return }
        refresh()
    }

    @discardableResult
    func refresh() -> Bool {
        guard pending == nil, let session, HvfGuestStartApps.unavailableReason(session) == nil else { return false }
        let request = HvfGuestStartAppsRequest(now: session.guestNow())
        beginReading(session)
        guard session.sendCtl(request.command) else {
            status = .failed(HvfGuestStartApps.notSent)
            endReadingIfIdle()
            return false
        }
        pending = request
        status = .loading
        return true
    }

    /// False when the request did not go to the guest.
    func launch(_ app: HvfGuestStartApp) -> Bool {
        guard let session, HvfGuestStartApps.unavailableReason(session) == nil,
              let launch = HvfGuestStartAppLaunch(app: app, now: session.guestNow()) else { return false }
        beginReading(session)
        guard session.sendCtl(launch.command) else { endReadingIfIdle(); return false }
        launches.append(launch)
        return true
    }

    func poll() {
        guard let session, let reader else { return }
        let lines = reader.readNewLines(from: Self.logURL(session))
        let now = session.guestNow()
        if let result = pending?.consume(lines: lines, now: now) {
            pending = nil
            switch result {
            case let .success(list):
                apps = list
                loadedAt = Date()
                status = .loaded
            case .failure(.unavailable): status = .failed(HvfGuestStartApps.listUnavailable)
            case .failure(.expired): status = .failed(HvfGuestStartApps.noAnswer)
            case .failure(.restarted): status = .idle // asked again when the menu opens
            }
        }
        launches.removeAll { launch in
            guard let answer = launch.consume(lines: lines, now: now) else { return false }
            if answer == .refused { onNotice?(HvfGuestStartApps.launchFailed(launch.app.name)) }
            return true
        }
        endReadingIfIdle()
    }

    private static func logURL(_ session: HvfEngineSession) -> URL {
        URL(fileURLWithPath: session.config.evidenceDir).appendingPathComponent("run.log")
    }

    private func beginReading(_ session: HvfEngineSession) {
        guard reader == nil else { return }
        let size = (try? FileManager.default.attributesOfItem(atPath: Self.logURL(session).path)[.size] as? NSNumber)?.uint64Value
        reader = TailOffsetReader(startingAt: size ?? 0)
        // The common modes include the menu's event tracking: the list fills in while the menu that asked is open.
        // A main run-loop timer fires on the main thread, so it reads at once (a Task would wait for the menu).
        let timer = Timer(timeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.poll() }
        }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func endReadingIfIdle() {
        guard pending == nil, launches.isEmpty else { return }
        timer?.invalidate()
        timer = nil
        reader = nil
    }
}
