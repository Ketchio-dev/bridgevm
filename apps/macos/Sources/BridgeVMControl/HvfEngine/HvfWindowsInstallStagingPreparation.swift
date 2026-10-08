import Foundation

extension HvfWindowsInstallStaging {
    static func prepareValidated(_ plan: HvfWindowsInstallPlan, targetBytes: UInt64) throws {
        let seed = try HvfWindowsBootSeed.bundledSeed()
        let metadata = try HvfWindowsInstallPrivateDirectory.metadata(of: URL(fileURLWithPath: plan.bundlePath))
        try metadata.removeTree(directoryName)
        let staging = try metadata.makeDirectory(directoryName)
        _ = try staging.makeDirectory(evidenceName)
        try staging.createFile(varsName) { try $0.write(contentsOf: seed) }
        try staging.createFile(targetName) { try $0.truncate(atOffset: targetBytes) }
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        var url = staging.url
        try? url.setResourceValues(values)
    }
}
