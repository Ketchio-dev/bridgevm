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
        for api in [F.API.getPID, .setMessagingTimeout, .getAttributeValueCount, .copyAttributeValues] {
            let f = F(); f.array = [f.child]; f.fail = api
            try failure(f, api: api, attribute: .windows) { _ = try f.access().related(f.parent, "AXWindows") }
        }
        for api in [F.API.getPID, .setMessagingTimeout] {
            let f = F(); f.fail = api; f.occurrence = 3; f.array = [f.child]
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
            let f = F(); f.array = [f.child]; f.fail = .copyAttributeValues; f.error = raw
            try failure(f, api: .copyAttributeValues, attribute: .windows) { _ = try f.access().related(f.parent, "AXWindows") }
        }
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
