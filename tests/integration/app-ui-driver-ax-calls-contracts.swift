import ApplicationServices
import Foundation
import Darwin

@main
enum AppUIDriverAXCallsContracts {
    typealias F = AppUIDriverAXCallsFixture
    typealias T = AppUIDriverProtocolTestSupport
    static func failure(_ fixture: F, api: F.API, attribute: AppUIDriverAXContext.Attribute,
                        scope: AppUIDriverAXContext.Scope = .element, _ operation: () throws -> Void) throws {
        do { try operation(); throw NSError(domain: "expected AX refusal", code: 1) }
        catch let error as AppUIDriverAXError {
            try T.check(error.failure == .axFailure && error.rawValue == fixture.error.rawValue, "raw AX failure preserved")
            try T.check(error.context == .init(api: api, attribute: attribute, scope: scope), "exact bounded AX call attribution")
            try T.check(fixture.calls.last == api, "no retry or later API after failure")
        }
    }
    static func refusal(_ code: AppUIDriverFailure, _ operation: () throws -> Void) throws {
        do { try operation(); throw NSError(domain: "expected refusal", code: 1) }
        catch let error as AppUIDriverFailure { try T.check(error == code, "original non-AX refusal preserved") }
    }
    static func failures() throws {
        for api in [F.API.getPID, .setMessagingTimeout, .copyAttributeValues] {
            let f = F(); f.fail = api
            try failure(f, api: api, attribute: .windows) { _ = try f.access().related(f.parent, "AXWindows") }
        }
        for api in [F.API.getPID, .setMessagingTimeout] {
            let f = F(); f.fail = api; f.occurrence = 2; f.array = [f.child]
            try failure(f, api: api, attribute: .children, scope: .relatedElement) {
                _ = try f.access().related(f.parent, "AXChildren")
            }
        }
        let names: [(String, AppUIDriverAXContext.Attribute)] = [("AXIdentifier", .identifier), ("AXRole", .role),
            ("AXTitle", .title), ("AXDescription", .description), ("AXValue", .value), ("private UI text", .other)]
        for (name, expected) in names {
            let f = F(); f.fail = .copyAttributeValue
            try failure(f, api: .copyAttributeValue, attribute: expected) { _ = try f.access().string(f.parent, name) }
            let context = AppUIDriverAXContext(api: .copyAttributeValue, attribute: .init(name), scope: .element)
            try T.check(!String(decoding: AppUIDriverCanonicalJSON.encode(context), as: UTF8.self).contains("private UI text"), "context excludes arbitrary strings")
        }
        let enabled = F(); enabled.fail = .copyAttributeValue
        try failure(enabled, api: .copyAttributeValue, attribute: .enabled) { _ = try enabled.access().enabled(enabled.parent) }
        let press = F(); press.fail = .performAction
        try failure(press, api: .performAction, attribute: .none) { try press.access().press(press.parent) }
        for api in [F.API.getPID, .setMessagingTimeout, .isAttributeSettable, .setAttributeValue] {
            let f = F(); f.fail = api
            try failure(f, api: api, attribute: .value) { try f.access().setSearch(f.parent) }
        }
        for api in [F.API.getPID, .setMessagingTimeout] {
            let f = F(); f.fail = api; f.occurrence = 2
            try failure(f, api: api, attribute: .value) { try f.access().setSearch(f.parent) }
            try T.check(f.assigned.isEmpty, "second preparation refusal prevents mutation")
        }
        for raw in [AXError.cannotComplete, .apiDisabled, .failure] {
            let f = F(); f.fail = .copyAttributeValues; f.error = raw
            try failure(f, api: .copyAttributeValues, attribute: .windows) { _ = try f.access().related(f.parent, "AXWindows") }
        }
    }
    static func boundaries() throws {
        for count in [0, 1, 10_000, 10_001] {
            let f = F(); f.array = Array(repeating: f.child, count: count)
            if count > 10_000 {
                try refusal(.protocolLimit) { _ = try f.access().related(f.parent, "AXChildren") }
                try T.check(f.calls == [.getPID, .setMessagingTimeout, .copyAttributeValues], "oversize refuses before traversing prefix")
            } else {
                let result = try f.access().related(f.parent, "AXChildren")
                try T.check(result.count == count, "array cardinality retained without deduplication")
                try T.check(f.calls.count == 3 + 2 * count, "every returned node receives ownership checks")
            }
            try T.check(f.ranges.count == 1 && f.ranges[0].0 == 0 && f.ranges[0].1 == 10_001, "existing sentinel range unchanged")
        }
        for error in [AXError.noValue, .attributeUnsupported] {
            let f = F(); f.fail = .copyAttributeValues; f.error = error
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
            let f = F(); f.afterCall = { api in if api == .copyAttributeValues { if cancel { f.cancelled = true } else { f.time = 105 } } }
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
    static func main() {
        do {
            try failures(); try boundaries()
            print("PASS: \(T.checks) production AX wrapper checks with injected messaging; no GUI or permission query")
        } catch {
            FileHandle.standardError.write(Data("FAIL near '\(T.lastCheck)': \(error)\n".utf8)); exit(1)
        }
    }
}
