import ApplicationServices
import Foundation

extension AppUIDriverAXEmptyContracts {
    static func refuses(_ expected: AppUIDriverFailure, _ action: () throws -> Void) throws {
        do { try action(); throw NSError(domain: "expected refusal", code: 1) }
        catch let error as AppUIDriverFailure {
            try T.check(error == expected, "exact non-AX refusal")
        }
    }
    static func axRefusal(_ fixture: F, api: F.API) throws {
        do {
            _ = try fixture.access().related(fixture.parent, "AXChildren")
            throw NSError(domain: "expected AX refusal", code: 1)
        } catch let error as AppUIDriverAXError {
            try T.check(error.rawValue == fixture.error.rawValue, "raw error is not replaced")
            let context = AppUIDriverAXContext(api: api, attribute: .children, scope: .element)
            try T.check(error.context == context, "count and copy failures have distinct attribution")
            try context.validate(outcome: .refused, failure: .axFailure, rawValue: error.rawValue)
            let encoded = try AppUIDriverCanonicalJSON.encode(context)
            try T.check(AppUIDriverCanonicalJSON.decode(encoded, as: AppUIDriverAXContext.self) == context,
                        "count context round trips without UI values")
            try T.check(fixture.calls.last == api, "failure stops without retry or later messages")
        }
    }
    static func countFailures() throws {
        for raw in [AXError.illegalArgument, .invalidUIElement, .cannotComplete, .apiDisabled, .failure, .notImplemented] {
            for api in [F.API.getAttributeValueCount, .copyAttributeValues] {
                let f = F(); f.array = [f.child]; f.fail = api; f.error = raw
                try axRefusal(f, api: api)
                if api == .getAttributeValueCount {
                    try T.check(f.ranges.isEmpty, "count failure cannot fall back to copying")
                }
            }
        }
        for raw in [AXError.noValue, .attributeUnsupported] {
            let f = F(); f.fail = .getAttributeValueCount; f.error = raw
            try T.check(f.access().related(f.parent, "AXChildren").isEmpty, "absent relationship remains absent")
            try T.check(f.ranges.isEmpty, "absent count avoids indexed copy")
        }
    }
    static func countBounds() throws {
        for count in [-1, 10_001, Int.max] {
            let f = F(); f.reportedCount = count
            try refuses(count < 0 ? .invalidElement : .protocolLimit) {
                _ = try f.access().related(f.parent, "AXChildren")
            }
            try T.check(f.ranges.isEmpty, "invalid count fails before copying")
        }
        let unset = F(); var access = unset.access()
        access.calls.relationships.count = { _, _, _ in .success }
        try refuses(.invalidElement) { _ = try access.related(unset.parent, "AXChildren") }
        let grown = F(); grown.reportedCount = 1; grown.array = Array(repeating: grown.child, count: 10_001)
        try refuses(.protocolLimit) { _ = try grown.access().related(grown.parent, "AXChildren") }
        try T.check(grown.ranges.count == 1 && grown.ranges[0].1 == 10_001, "growth retains original oversize sentinel")
        try T.check(grown.calls.count == 6, "oversized copy refuses before validating a partial prefix")
        let shrunk = F(); shrunk.reportedCount = 1
        try T.check(shrunk.access().related(shrunk.parent, "AXChildren").isEmpty, "successful empty copy after shrink is valid")
        let vanished = F(); vanished.reportedCount = 1; vanished.fail = .copyAttributeValues
        vanished.error = .illegalArgument
        try axRefusal(vanished, api: .copyAttributeValues)
    }
    static func countLifetime() throws {
        for empty in [false, true] {
            for cancel in [false, true] {
                let f = F(); f.array = empty ? [] : [f.child]
                f.afterCall = { api in
                    if api == .getAttributeValueCount { if cancel { f.cancelled = true } else { f.time = 105 } }
                }
                try refuses(cancel ? .cancelled : .deadlineExceeded) {
                    _ = try f.access().related(f.parent, "AXChildren")
                }
                try T.check(f.ranges.isEmpty, "late count is never accepted, even when empty")
            }
        }
        let owner = F(); owner.array = [owner.child]
        owner.afterCall = { if $0 == .getAttributeValueCount { owner.observedPID = 99 } }
        try refuses(.identityMismatch) { _ = try owner.access().related(owner.parent, "AXChildren") }
        try T.check(owner.ranges.isEmpty, "copy must revalidate the parent after counting")
        let timeout = F(); timeout.array = [timeout.child]
        timeout.afterCall = { if $0 == .getAttributeValueCount { timeout.time = 104.875 } }
        _ = try timeout.access().related(timeout.parent, "AXChildren")
        try T.check(timeout.timeouts == [0.25, 0.125, 0.125], "count does not reset deadline or per-message timeout")
    }
    static func copiedShape() throws {
        for malformed in [false, true] {
            let f = F(); f.array = [f.child]; var access = f.access()
            access.calls.relationships.read = { _, _, _, _, output in
                output.pointee = malformed ? ["not an AX element"] as CFArray : nil
                return .success
            }
            try refuses(.invalidElement) { _ = try access.related(f.parent, "AXChildren") }
        }
        let foreign = F(); foreign.array = [foreign.child]
        foreign.afterCall = { if $0 == .copyAttributeValues { foreign.observedPID = 99 } }
        try refuses(.identityMismatch) { _ = try foreign.access().related(foreign.parent, "AXChildren") }
    }
    static func countBoundaries() throws {
        try countFailures(); try countBounds(); try countLifetime(); try copiedShape()
    }
}
