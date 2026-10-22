import ApplicationServices
import Foundation

extension AppUIDriverAXCallsContracts {
    static func boundaries() throws {
        for count in [0, 1, 10_000, 10_001] {
            let f = F(); f.array = Array(repeating: f.child, count: count)
            if count > 10_000 {
                try refusal(.protocolLimit) { _ = try f.access().related(f.parent, "AXChildren") }
                try T.check(f.calls == [.getPID, .setMessagingTimeout, .getAttributeValueCount], "oversize refuses before traversing prefix")
            } else {
                let result = try f.access().related(f.parent, "AXChildren")
                try T.check(result.count == count, "array cardinality retained without deduplication")
                try T.check(f.calls.count == (count == 0 ? 3 : 6 + 2 * count), "every returned node receives ownership checks")
            }
            if count == 0 || count > 10_000 {
                try T.check(f.ranges.isEmpty, "empty or oversized count prevents indexed copy")
            } else {
                try T.check(f.ranges.count == 1 && f.ranges[0].0 == 0 && f.ranges[0].1 == 10_001, "existing sentinel range unchanged")
            }
        }
        for error in [AXError.noValue, .attributeUnsupported] {
            let f = F(); f.array = [f.child]; f.fail = .copyAttributeValues; f.error = error
            try T.check(f.access().related(f.parent, "AXWindows").isEmpty, "existing absent relationship behavior")
            let g = F(); g.fail = .copyAttributeValue; g.error = error
            try T.check(g.access().string(g.parent, "AXIdentifier") == nil, "existing absent attribute behavior")
        }
        let mismatch = F(); mismatch.observedPID = 99
        try refusal(.identityMismatch) { _ = try mismatch.access().related(mismatch.parent, "AXWindows") }
        try T.check(mismatch.calls == [.getPID], "owner mismatch prevents messaging")
        for deadline in [100.0, Double.nan] {
            let f = F()
            try refusal(.deadlineExceeded) { _ = try f.access(deadline: deadline).related(f.parent, "AXWindows") }
            try T.check(f.calls.isEmpty, "expired deadline prevents messaging")
        }
        let cancelled = F(); cancelled.cancelled = true
        try refusal(.cancelled) { try cancelled.access().press(cancelled.parent) }
        try T.check(cancelled.calls.isEmpty, "cancellation prevents mutation")
        for cancel in [false, true] {
            let f = F(); f.array = [f.child]; f.afterCall = { api in if api == .copyAttributeValues { if cancel { f.cancelled = true } else { f.time = 105 } } }
            try refusal(cancel ? .cancelled : .deadlineExceeded) { _ = try f.access().related(f.parent, "AXWindows") }
        }
        for api in [F.API.getPID, .setMessagingTimeout] {
            let f = F(); f.afterCall = { if $0 == api { f.time = 105 } }
            try refusal(.deadlineExceeded) { _ = try f.access().related(f.parent, "AXWindows") }
            try T.check(f.calls.last == api, "deadline during preparation prevents attribute query")
        }
        let short = F(); _ = try short.access(deadline: 100.125).related(short.parent, "AXWindows")
        try T.check(short.timeouts == [0.125], "messaging timeout stays within remaining deadline")
        let success = F(); try success.access().press(success.parent); try success.access().setSearch(success.parent)
        try T.check(success.actions == ["AXPress"] && success.assigned == ["Indigo"], "fixed mutation values unchanged")
        let readonly = F(); readonly.writable = false
        try refusal(.invalidElement) { try readonly.access().setSearch(readonly.parent) }
        try T.check(readonly.assigned.isEmpty, "readonly value refuses mutation")
    }
}
