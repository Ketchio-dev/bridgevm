import Foundation

/// Serial dispatch; an insertion receipt never proves application consumption.
struct HvfAcknowledgedInputStream {
    enum Update: Equatable {
        case idle, waiting
        case sent(UUID), inserted(UUID)
        case cancelled(HvfOrderedInputQueue.Failure, discarded: Int)
    }

    private var queue = HvfOrderedInputQueue()
    private var pending: (ticket: HvfOrderedInputQueue.Ticket, request: HvfUnicodeInputRequest)?
    var count: Int { queue.count }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> Bool {
        guard HvfGuestInputEncoding(event) != nil else { return false }
        return queue.enqueue(event, now: now)
    }

    mutating func advance(lines: [String], now: Date, send: (String) -> Bool) -> Update {
        // A restart cancels even a batch whose earlier lines contain a valid receipt.
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || $0.hasPrefix("PSCI_SYSTEM_RESET")
        }) { return cancel(.sessionChanged) }
        switch queue.poll(now: now) {
        case let .send(ticket):
            guard let request = HvfUnicodeInputRequest(event: ticket.event, now: now, id: ticket.id) else {
                return cancel(.transportFailed)
            }
            pending = (ticket, request)
            guard send(request.command) else { return cancel(.transportFailed) }
            // Lines collected before this send cannot acknowledge this new request.
            return .sent(ticket.id)
        case .waiting:
            guard var active = pending else { return cancel(.transportFailed) }
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
                guard transition == .completed else { return cancel(.transportFailed) }
                return .inserted(active.ticket.id)
            case let .failed(failure):
                switch failure {
                case .expired: return cancel(.expired)
                case .restarted: return cancel(.sessionChanged)
                case .guestRejected, .invalidReceipt: return cancel(.transportFailed)
                }
            }
        case let .cancelled(reason, discarded):
            pending = nil
            return .cancelled(reason, discarded: discarded)
        case .idle:
            return .idle
        case .completed, .ignored:
            return cancel(.transportFailed)
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
