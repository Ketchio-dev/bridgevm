import Foundation

/// Serial dispatch; an insertion receipt never proves application consumption.
struct HvfAcknowledgedInputStream {
    enum Update: Equatable {
        case idle, waiting
        case sent(UUID), inserted(UUID)
        case cancelled(HvfOrderedInputQueue.Failure, discarded: Int)
    }

    private struct Pending {
        let ticket: HvfOrderedInputQueue.Ticket
        var request: HvfUnicodeInputRequest?
        var remaining: ArraySlice<Unicode.Scalar>
        var insertedPrefix = false
        var continueAfter = Date.distantPast
    }
    private var queue = HvfOrderedInputQueue()
    private var pending: Pending?
    var count: Int { queue.count }

    mutating func enqueue(_ event: HvfOrderedInputQueue.Event, now: Date) -> Bool {
        guard HvfGuestInputEncoding(event) != nil else { return false }
        return queue.enqueue(event, now: now)
    }

    mutating func advance(lines: [String], now: Date, send: (String) -> Bool) -> Update {
        // A restart cancels even a batch whose earlier lines contain a valid receipt.
        if lines.contains(where: {
            $0.hasPrefix("BVAGENT READY") || $0.hasPrefix("BVAGENT re-READY") ||
            $0.hasPrefix("BVAGENT SERVICE start") || HvfGuestResetRecord.matches($0)
        }) { return cancel(.sessionChanged) }
        switch queue.poll(now: now) {
        case let .send(ticket):
            var event = ticket.event
            var remaining: ArraySlice<Unicode.Scalar> = []
            if case let .text(text) = event {
                // WinUI Notepad corrupted bulk VK_PACKET text despite complete insertion receipts.
                // Pace complete Unicode scalars by receipts, keeping the commit in one queue slot.
                remaining = ArraySlice(text.unicodeScalars)
                guard let first = remaining.popFirst() else { return cancel(.transportFailed) }
                event = .text(String(first))
            }
            guard let request = HvfUnicodeInputRequest(event: event, now: now, id: ticket.id) else {
                return cancel(.transportFailed)
            }
            pending = Pending(ticket: ticket, request: request, remaining: remaining)
            guard send(request.command) else { return cancel(.transportFailed) }
            // Lines collected before this send cannot acknowledge this new request.
            return .sent(ticket.id)
        case .waiting:
            guard var active = pending else { return cancel(.transportFailed) }
            if active.request == nil {
                guard now >= active.continueAfter else { return .waiting }
                guard let next = active.remaining.popFirst() else { return cancel(.transportFailed) }
                let id = UUID()
                guard let request = HvfUnicodeInputRequest(event: .text(String(next)), now: now, id: id) else {
                    return cancel(.transportFailed)
                }
                active.request = request
                pending = active
                guard send(request.command) else { return cancel(.transportFailed) }
                // This continues the same logical event; the recovery ledger must not pop another entry.
                return .waiting
            }
            guard let outcome = active.request?.consume(lines: lines, now: now) else {
                pending = active
                return .waiting
            }
            switch outcome {
            case .inserted:
                if !active.remaining.isEmpty {
                    active.insertedPrefix = true
                    active.request = nil
                    // Receipts alone at the 10 ms pump interval still corrupted complex text live.
                    active.continueAfter = now.addingTimeInterval(0.05)
                    pending = active
                    // The next poll sends the next scalar; never reuse this receipt batch.
                    return .waiting
                }
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
                // A later scalar inserting nothing cannot make the already inserted prefix replayable.
                case .notInserted: return cancel(active.insertedPrefix ? .transportFailed : .notInserted)
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
