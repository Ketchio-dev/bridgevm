import Foundation
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// The caret question in flight, at most one per session, and the 10 ms poll that reads its answer sooner than
/// the session's 100 ms tick would (the overlay moves to the caret as soon as the guest says where it is).
@MainActor
final class HvfGuestCaretQuery {
    var request: HvfGuestCaretRequest
    let answer: (CGRect?) -> Void
    var timer: Timer?

    init(request: HvfGuestCaretRequest, answer: @escaping (CGRect?) -> Void) {
        self.request = request
        self.answer = answer
    }

    deinit { timer?.invalidate() }
}

extension HvfEngineSession {
    /// Whether the guest agent says where its text caret is (the INPUTCAPS marker; an older agent is never asked).
    var guestCaret: Bool { inputDriver.router.caret }

    /// Asks the guest agent where its text caret is, never waiting for it: `answer` later gets the caret in guest
    /// pixels, or nil for none, an error, a guest restart or no answer within HvfGuestCaretRequest.timeout.
    /// False when not asked: an agent without the verb, no connected guest service, or a question in flight.
    func requestGuestCaret(_ answer: @escaping (CGRect?) -> Void) -> Bool {
        guard guestCaretQuery == nil, guestCaret, serviceStarted, case .connected = connectionState,
              !pause.holdsInput else { return false }
        let request = HvfGuestCaretRequest(now: hostNow())
        guard sendCtl(request.command) else { return false }
        let query = HvfGuestCaretQuery(request: request, answer: answer)
        query.timer = Timer.scheduledTimer(withTimeInterval: 0.01, repeats: true) { [weak self] _ in
            Task { @MainActor in
                // The poll's lines answer it; the empty batch ends it at the deadline even when poll() returned early.
                self?.poll()
                self?.answerGuestCaret(lines: [])
            }
        }
        guestCaretQuery = query
        return true
    }

    /// run.log's new lines, from poll().
    func answerGuestCaret(lines: [String]) {
        guard let query = guestCaretQuery, let answer = query.request.consume(lines: lines, now: hostNow()) else { return }
        guestCaretQuery = nil
        query.timer?.invalidate()
        if case let .caret(rect) = answer { query.answer(rect) } else { query.answer(nil) }
    }
}
