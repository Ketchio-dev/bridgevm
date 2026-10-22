import ApplicationServices
import Foundation
import Darwin

@main
enum AppUIDriverAXEmptyContracts {
    typealias F = AppUIDriverAXCallsFixture
    typealias T = AppUIDriverProtocolTestSupport

    static func emptyRelationships() throws {
        for name in ["AXChildren", "AXWindows"] {
            let fixture = F()
            // Index zero is outside an empty array. Model an AX provider that
            // refuses that indexed read rather than returning a successful [].
            fixture.fail = .copyAttributeValues
            fixture.error = .illegalArgument
            let result = try fixture.access().related(fixture.parent, name)
            try T.check(result.isEmpty, "empty relationship is a valid leaf")
            try T.check(fixture.ranges.isEmpty, "empty relationship never requests index zero")
        }
    }

    static func main() {
        do {
            try emptyRelationships(); try countBoundaries()
            print("PASS: \(T.checks) empty AX relationship checks; injected calls only")
        } catch {
            FileHandle.standardError.write(Data("FAIL: empty relationship: \(error)\n".utf8))
            exit(1)
        }
    }
}
