import XCTest
@testable import BridgeVMControl

final class VMResourceLimitsTests: XCTestCase {
    func testSharedResourceLimitsMatchSupportedUIRange() {
        XCTAssertTrue(VMResourceLimits.contains(memoryMiB: 1_024, cpu: 1))
        XCTAssertTrue(VMResourceLimits.contains(memoryMiB: 32 * 1_024, cpu: 10))
        XCTAssertFalse(VMResourceLimits.contains(memoryMiB: 1_023, cpu: 1))
        XCTAssertFalse(VMResourceLimits.contains(memoryMiB: 32 * 1_024 + 1, cpu: 10))
        XCTAssertFalse(VMResourceLimits.contains(memoryMiB: 4_096, cpu: VMResourceLimits.maximumCPU + 1))
        // Create and the CLI offer all cores but one; Settings must accept what they made.
        XCTAssertTrue(VMResourceLimits.contains(memoryMiB: 4_096,
                                                cpu: max(1, ProcessInfo.processInfo.activeProcessorCount - 1)))
    }
}
