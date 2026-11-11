import Foundation
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// The caret question, at most one per session, and the 10 ms poll that reads its answer sooner than the session's
/// 100 ms tick would (the overlay moves to the caret as soon as the guest says where it is). It goes to the agent
/// only once ordered input queued before it has gone in, since that input moves the caret (a Korean syllable
/// committed as the next one starts).
@MainActor
final class HvfGuestCaretQuery {
    /// Ordered input ahead of the question has this long to go in; then the question ends unanswered.
    static let drainLimit: TimeInterval = 1
    /// The question on the wire; nil while ordered input ahead of it is still going in.
    var request: HvfGuestCaretRequest?
    /// The latest asker. Asked again while a question was on the wire, its answer may predate input sent since:
    /// `again` drops that answer and asks once more.
    var answer: (CGRect?) -> Void
    var again = false
    var asked: Date
    var timer: Timer?

    init(asked: Date, answer: @escaping (CGRect?) -> Void) {
        self.asked = asked
        self.answer = answer
    }

    deinit { timer?.invalidate() }
}

extension HvfEngineSession {
    /// Whether the guest agent says where its text caret is (the INPUTCAPS marker; an older agent is never asked).
    var guestCaret: Bool { inputDriver.router.caret }

    private var guestCaretAskable: Bool {
        guard guestCaret, serviceStarted, case .connected = connectionState else { return false }
        return !pause.holdsInput
    }

    /// Asks the guest agent where its text caret is, never waiting for it: `answer` later gets the caret in guest
    /// pixels, or nil for none, an error, a guest restart, ordered input that did not go in within
    /// HvfGuestCaretQuery.drainLimit, or no answer within HvfGuestCaretRequest.timeout. A question already under
    /// way answers the new asker instead. False when not asked: an agent without the verb or no connected service.
    func requestGuestCaret(_ answer: @escaping (CGRect?) -> Void) -> Bool {
        guard guestCaretAskable else { return false }
        if let query = guestCaretQuery {
            query.answer = answer
            if query.request != nil { query.again = true }
            return true
        }
        let query = HvfGuestCaretQuery(asked: hostNow(), answer: answer)
        query.timer = Timer.scheduledTimer(withTimeInterval: 0.01, repeats: true) { [weak self] _ in
            Task { @MainActor in
                // The poll's lines answer it (and send it once input ahead went in); the empty batch ends it at
                // the deadline even when poll() returned early.
                self?.poll()
                self?.answerGuestCaret(lines: [])
            }
        }
        guestCaretQuery = query
        sendDueGuestCaret()
        return true
    }

    /// From poll(), after the input driver read its receipts: the question goes once no ordered input is ahead.
    func sendDueGuestCaret() {
        guard let query = guestCaretQuery, query.request == nil else { return }
        let now = hostNow()
        guard guestCaretAskable else { return finishGuestCaret(nil) }
        guard inputDriver.router.count == 0 else {
            if now.timeIntervalSince(query.asked) >= HvfGuestCaretQuery.drainLimit { finishGuestCaret(nil) }
            return
        }
        let request = HvfGuestCaretRequest(now: now)
        guard sendCtl(request.command) else { return finishGuestCaret(nil) }
        query.request = request
    }

    /// run.log's new lines, from poll().
    func answerGuestCaret(lines: [String]) {
        guard let query = guestCaretQuery, query.request != nil else { return sendDueGuestCaret() }
        let now = hostNow()
        guard let answer = query.request?.consume(lines: lines, now: now) else { return }
        // Asked again meanwhile: an answer may predate input sent since, so ask once more (not after a timeout).
        if query.again, let deadline = query.request?.deadline, now < deadline {
            query.again = false
            query.request = nil
            query.asked = now
            return sendDueGuestCaret()
        }
        if case let .caret(rect) = answer { finishGuestCaret(rect) } else { finishGuestCaret(nil) }
    }

    private func finishGuestCaret(_ rect: CGRect?) {
        guard let query = guestCaretQuery else { return }
        guestCaretQuery = nil
        query.timer?.invalidate()
        query.answer(rect)
    }
}
