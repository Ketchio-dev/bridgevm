import Darwin
import Foundation

/// Registered while the nonce-bound fixture child is alive; never signals a PID.
final class HvfOwnedRunnerExitWitness {
    private let descriptor: Int32
    private(set) var exited = false

    init(pid: Int32) throws {
        descriptor = kqueue()
        guard descriptor >= 0 else { throw POSIXError(.EIO) }
        var event = kevent(ident: UInt(pid), filter: Int16(EVFILT_PROC),
            flags: UInt16(EV_ADD | EV_ENABLE | EV_ONESHOT), fflags: UInt32(NOTE_EXIT),
            data: 0, udata: nil)
        guard kevent(descriptor, &event, 1, nil, 0, nil) == 0 else {
            Darwin.close(descriptor)
            throw POSIXError(.ESRCH)
        }
    }

    func observe() -> Bool {
        if exited { return true }
        var event = kevent(), timeout = timespec(tv_sec: 0, tv_nsec: 0)
        if kevent(descriptor, nil, 0, &event, 1, &timeout) == 1 {
            exited = event.fflags & UInt32(NOTE_EXIT) != 0
        }
        return exited
    }

    deinit { Darwin.close(descriptor) }
}
