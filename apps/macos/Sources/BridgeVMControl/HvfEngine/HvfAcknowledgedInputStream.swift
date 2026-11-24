import Foundation

/// Serial dispatch; an insertion receipt never proves application consumption.
struct HvfAcknowledgedInputStream {
    enum Update: Equatable {
        case idle, waiting
        case sent(UUID), inserted(UUID, draft: HvfTextDraftDelivery? = nil)
        case cancelled(HvfOrderedInputQueue.Failure, discarded: Int)
    }

    private var queue = HvfOrderedInputQueue()
    private var pending: (ticket: HvfOrderedInputQueue.Ticket, request: HvfUnicodeInputRequest)?
    var count: Int { queue.count }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> Bool {
        guard HvfGuestInputEncoding(event) != nil else { return false }
        return queue.enqueue(event, now: now)
    }

    mutating func advance(lines: [String], now: Date, queuedTextDispatch: Bool = false, send: (String) -> Bool) -> Update {
        // A restart cancels even a batch whose earlier lines contain a valid receipt.
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) { return cancel(.sessionChanged) }
        switch queue.poll(now: now) {
        case let .send(ticket):
            guard let request = HvfUnicodeInputRequest(event: ticket.event, now: now, id: ticket.id, queuedTextDispatch: queuedTextDispatch) else {
                assertionFailure("enqueue admits only encodable events")
                return cancel(.transportFailed)
            }
            pending = (ticket, request)
            guard send(request.command) else { return cancel(.transportFailed) }
            // Lines collected before this send cannot acknowledge this new request.
            return .sent(ticket.id)
        case .waiting:
            guard var active = pending else {
                assertionFailure("the queue waits only on a request this stream sent")
                return cancel(.transportFailed)
            }
            guard let outcome = active.request.consume(lines: lines, now: now) else {
                pending = active
                return .waiting
            }
            switch outcome {
            case .inserted:
                pending = nil
                let transition = queue.acknowledge(active.ticket, succeeded: true, now: now)
                if case let .cancelled(reason, discarded) = transition {
                    return .cancelled(reason, discarded: discarded)
                }
                assert(transition == .completed, "the active ticket is always acknowledged")
                return .inserted(active.ticket.id, draft: active.ticket.event.draft)
            case .unsupported:
                guard active.request.queuedTextDispatch else { return cancel(.transportFailed) }
                if active.ticket.event.draft != nil { return cancel(.unsupportedEditor) }
                guard let fallback = HvfUnicodeInputRequest(event: active.ticket.event, now: now) else { return cancel(.transportFailed) }
                active.request = fallback
                pending = active
                // The guest positively dispatched no text. Keep the same logical event,
                // with a fresh receipt ID, rather than popping the recovery ledger again.
                guard send(fallback.command) else { return cancel(.transportFailed) }
                return .waiting
            case let .failed(failure):
                switch failure {
                case .expired: return cancel(.expired)
                case .restarted: return cancel(.sessionChanged)
                case .guestRejected, .invalidReceipt: return cancel(.transportFailed)
                case .notInserted: return cancel(.notInserted)
                }
            }
        case let .cancelled(reason, discarded):
            pending = nil
            return .cancelled(reason, discarded: discarded)
        case .idle:
            return .idle
        case .completed, .ignored:
            assertionFailure("poll never completes or ignores a ticket")
            return .idle
        }
    }

    @discardableResult
    mutating func cancel(_ reason: HvfOrderedInputQueue.Failure) -> Update {
        pending = nil
        let transition = queue.cancel(reason)
        if case let .cancelled(failure, discarded) = transition {
            return .cancelled(failure, discarded: discarded)
        }
        return .cancelled(reason, discarded: 0)
    }
}
