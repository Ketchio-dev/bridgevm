import Foundation

enum HvfMediaImportTestSupport {
    /// The harness builds this real helper; missing native coverage is a failure.
    static var helper: URL {
        if let path = ProcessInfo.processInfo.environment["BRIDGEVM_TEST_SNAPSHOT_HELPER"] {
            return URL(fileURLWithPath: path).resolvingSymlinksInPath()
        }
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        return repository.appendingPathComponent("target/debug/examples/snapshot_pair_cli")
            .resolvingSymlinksInPath()
    }
}
