import Foundation

/// Host input ordering only. The transport must acknowledge actual completion
/// and cancel this queue when the guest session or input target changes.
struct HvfOrderedInputQueue {
    struct Ticket: Equatable {
        let id: UUID
        let generation: UUID
        let event: Event
    }

    /// Legacy delivery requires that the awaited event was unsent or that Windows inserted nothing.
    enum Failure: Equatable {
        case expired, transportFailed, sessionChanged, targetChanged, notInserted, clipboardRequired

        var permitsLegacyRetry: Bool { self == .notInserted || self == .clipboardRequired }
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
        if active != nil, let last = entries.last, last.ticket.generation == generation,
           let merged = last.ticket.event.absorbing(event) {
            // Behind an awaited receipt the newest unsent move or scroll absorbs the newer one in place
            // (Event.absorbing): no new slot, a fresh id and the original deadline. A sent ticket is never modified.
            let others = byteCount - last.ticket.event.byteCount
            guard merged.byteCount <= Self.maximumBytes - others else { return false }
            entries[entries.count - 1] = Entry(ticket: Ticket(id: UUID(), generation: generation, event: merged),
                                               deadline: last.deadline)
            byteCount = others + merged.byteCount
            return true
        }
        // A release is never refused for room, or Windows would keep the key or button down: a key's comes once per
        // key held (HvfHeldKeys, at most six) and a button's once per mouse-up, so they add only a few short
        // events beyond the limits.
        guard bytes > 0, event.isRelease || (count < Self.maximumEvents && bytes <= Self.maximumBytes - byteCount)
        else { return false }
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
