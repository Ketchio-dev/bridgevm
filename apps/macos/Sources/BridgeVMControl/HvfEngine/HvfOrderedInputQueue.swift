import Foundation

/// Host input ordering only. The transport must acknowledge actual completion
/// and cancel this queue when the guest session or input target changes.
struct HvfOrderedInputQueue {
    struct Ticket: Equatable {
        let id: UUID
        let generation: UUID
        let event: Event
    }

    enum Failure: Equatable {
        case expired, transportFailed, sessionChanged, targetChanged
    }

    enum Transition: Equatable {
        case idle, waiting, completed, ignored
        case send(Ticket)
        case cancelled(Failure, discarded: Int)
    }

    private struct Entry {
        let ticket: Ticket
        let deadline: Date
    }

    static let maximumEvents = 64
    static let maximumBytes = 65_536
    private var generation = UUID()
    private var entries: [Entry] = []
    private var active: Entry?
    private(set) var byteCount = 0
    var count: Int { entries.count + (active == nil ? 0 : 1) }

    /// False means the caller must surface rejection; existing input is retained.
    mutating func enqueue(_ event: Event, now: Date) -> Bool {
        let bytes = event.byteCount
        if active != nil, event.isPointerMove, let last = entries.last, last.ticket.event.isPointerMove,
           last.ticket.generation == generation {
            // Behind an awaited receipt the newest unsent move takes the newer position in place:
            // no new slot, a fresh id and the original deadline. A sent ticket is never modified.
            let others = byteCount - last.ticket.event.byteCount
            guard bytes <= Self.maximumBytes - others else { return false }
            entries[entries.count - 1] = Entry(ticket: Ticket(id: UUID(), generation: generation, event: event),
                                               deadline: last.deadline)
            byteCount = others + bytes
            return true
        }
        guard bytes > 0, count < Self.maximumEvents,
              bytes <= Self.maximumBytes - byteCount else { return false }
        let ticket = Ticket(id: UUID(), generation: generation, event: event)
        entries.append(Entry(ticket: ticket, deadline: now.addingTimeInterval(30)))
        byteCount += bytes
        return true
    }

    /// A ticket is emitted once, never retransmitted while its outcome is unknown.
    mutating func poll(now: Date) -> Transition {
        if let active {
            return now < active.deadline ? .waiting : cancel(.expired)
        }
        guard let first = entries.first else { return .idle }
        guard now < first.deadline else { return cancel(.expired) }
        active = entries.removeFirst()
        return .send(first.ticket)
    }

    /// A write to agent.ctl is not completion of a clipboard-backed text input.
    mutating func acknowledge(_ ticket: Ticket, succeeded: Bool, now: Date) -> Transition {
        guard let active, active.ticket == ticket else { return .ignored }
        guard now < active.deadline else { return cancel(.expired) }
        guard succeeded else { return cancel(.transportFailed) }
        byteCount -= active.ticket.event.byteCount
        self.active = nil
        return .completed
    }

    @discardableResult
    mutating func cancel(_ reason: Failure) -> Transition {
        let discarded = count
        entries.removeAll(keepingCapacity: true)
        active = nil
        byteCount = 0
        generation = UUID()
        return .cancelled(reason, discarded: discarded)
    }
}
