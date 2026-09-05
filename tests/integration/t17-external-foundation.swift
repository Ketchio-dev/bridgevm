import Foundation

struct T17Blocker: Error { let code: String; let detail: String }

@main
enum ExternalStorageProbe {
    static func main() throws {
        let storage = URL(fileURLWithPath: CommandLine.arguments[1])
        let suffix = String(UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(6))
        let work = storage.appendingPathComponent("bridgevm-e2e-foundation.\(suffix)")
        let lane = work.appendingPathComponent("lane-1")
        let fm = FileManager.default
        try fm.createDirectory(at: work, withIntermediateDirectories: false)
        defer { try? fm.removeItem(at: work) }
        try fm.createDirectory(at: lane, withIntermediateDirectories: false)
        try T17StorageBoundary.validate(lane.path)
        let library = lane.appendingPathComponent("library")
        guard HvfWindowsInstallTemporaryPaths.validationError(libraryRoot: library) == nil,
              HvfWindowsInstallTemporaryPaths.prefix(libraryRoot: library, slug: "probe") == lane.path + "/bridgevm-appinstall-probe" else {
            throw T17Blocker(code: "failed", detail: "product storage validation disagrees")
        }
        print("PASS: packaged-helper and installer Foundation boundaries accept the real external volume; Windows install remains unproven")
    }
}
