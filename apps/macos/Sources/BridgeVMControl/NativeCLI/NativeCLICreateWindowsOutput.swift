import Foundation

struct NativeCLICreateWindowsOutput: Encodable, Equatable {
    let schema = "bridgevm.app-create-windows.v1"
    let id: String
    let displayName: String
    let backendKind: String
    let bootMode: String
    let installPending: Bool
    let cpuCount: Int
    let memoryMiB: Int
    let diskGiB: Int
    let resolution: String
    let networkEnabled: Bool
    let configurationDigest: String

    var text: String {
        "Created pending Windows VM \(id) (\(displayName)).\n" +
        "Saved resources: \(cpuCount) CPU, \(memoryMiB) MiB, \(diskGiB) GiB, \(resolution).\n" +
        "No installation or guest boot was run. Next: bridgevm app install \(id)\n"
    }
}
