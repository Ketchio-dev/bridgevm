import AppKit
import ApplicationServices
import Foundation

enum T17ActivationProbe {
    static func capture(pid: pid_t, timeout: TimeInterval, admitted: () -> Bool = { true }) -> T17ActivationRecord {
        T17ActivationNative.capture(pid: pid, timeout: timeout, admitted: admitted)
    }

    /// Same activation-only retries and 200 ms cadence. The monotonic deadline
    /// is cooperative: synchronous AX calls are not given a hard time bound.
    static func measure(
        timeout: TimeInterval, clock: () -> TimeInterval,
        isActive: () -> Bool?, activate: () -> Bool?, setFront: () -> Int32,
        readFront: () -> (Int32, Bool?), pause: (TimeInterval) -> Void,
        foreground: () -> pid_t?, admitted: () -> Bool = { true }
    ) -> T17ActivationRecord {
        let started = clock()
        var record = T17ActivationRecord()
        func finish(_ succeeded: Bool) -> T17ActivationRecord {
            record.succeeded = succeeded
            record.observedFrontPID = foreground()
            let elapsed = (clock() - started) * 1_000
            record.elapsedMilliseconds = elapsed.isFinite
                ? UInt64(min(4_294_967_295, max(0, elapsed))) : 0
            return record
        }
        guard timeout.isFinite, timeout >= 0, started.isFinite else { return finish(false) }
        let deadline = started + timeout
        repeat {
            guard admitted() else { return finish(false) }
            record.nativeActive = isActive()
            if record.nativeActive == true { return finish(true) }
            guard admitted() else { return finish(false) }
            record.attempts += 1
            record.nativeActivationAccepted = activate()
            guard admitted() else { return finish(false) }
            record.axSetCode = setFront()
            guard admitted() else { return finish(false) }
            pause(0.2)
            guard admitted() else { return finish(false) }
            let front = readFront()
            record.axReadCode = front.0
            record.axFront = front.1
            guard admitted() else { return finish(false) }
            record.nativeActive = isActive()
            if front.0 == AXError.success.rawValue, front.1 == true { return finish(true) }
        } while clock() < deadline
        guard admitted() else { return finish(false) }
        record.nativeActive = isActive()
        return finish(record.nativeActive == true)
    }
}
