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
    /// The pinned virtio-win payload from `--fetch-drivers` is sealed in the bundle.
    let guestDriversSealed: Bool
    let configurationDigest: String

    var text: String {
        "Created pending Windows VM \(id) (\(displayName)).\n" +
        "Saved resources: \(cpuCount) CPU, \(memoryMiB) MiB, \(diskGiB) GiB, \(resolution).\n" +
        (guestDriversSealed ? NativeCLICreateWindowsGuidance.sealedText(id: id) : NativeCLICreateWindowsGuidance.text(id: id))
    }
}
