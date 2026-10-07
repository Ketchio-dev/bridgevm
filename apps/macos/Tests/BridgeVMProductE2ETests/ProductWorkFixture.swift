import Foundation
import BridgeVMWindowProtocol

struct ProductWorkFixture {
    let parent: URL, work: URL, lane: URL
    let parentID: String, workID: String
    init(job: String, importing: Bool = false) throws {
        let fm = FileManager.default
        parent = fm.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("product-work-\(UUID().uuidString)")
        work = parent.appendingPathComponent("bridgevm-\(importing ? "import-e2e" : "e2e")-\(job).Ab12Cd")
        lane = work.appendingPathComponent("lane-1")
        for directory in [parent, work, lane] {
            try fm.createDirectory(at: directory, withIntermediateDirectories: false, attributes: [.posixPermissions: 0o700])
        }
        parentID = try ProductE2EWorkBoundary.identity(parent)
        workID = try ProductE2EWorkBoundary.identity(work)
    }
    var fields: [String: Any] {
        ["work_parent": parent.path, "work_parent_identity": parentID, "work_identity": workID]
    }
}
