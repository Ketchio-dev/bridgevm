import Foundation
import XCTest
@testable import BridgeVMControl

@MainActor
final class NativeRuntimeStartAdmissionTests: XCTestCase {
    private typealias F = NativeRuntimeStartTestSupport

    func testUnknownStatusDoesNotReadModelAndAMissingModelIsASentence() throws {
        var reads = 0
        let router = NativeRuntimeAppStartRouter(appInstanceID: F.app, library: F.library,
            retainedModel: { reads += 1; return nil })
        XCTAssertEqual(try router.handle(F.request(.startStatus)).refusal, .operationUnknown); XCTAssertEqual(reads, 0)
        let request = F.request()
        for _ in 0..<2 {
            let refused = try router.handle(request)
            XCTAssertEqual(refused.refusal, .refused); XCTAssertEqual(refused.reason?.isEmpty, false)
        }
        XCTAssertEqual(reads, 1, "a refused operation ID replays its refusal")
        XCTAssertThrowsError(try router.handle(F.request(vmID: "../vm")), "the VM ID is a path component")
    }
}
