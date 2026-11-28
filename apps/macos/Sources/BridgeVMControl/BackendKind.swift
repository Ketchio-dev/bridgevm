/// The engine a VM runs on — the single source of truth for the kind strings and
/// their display labels. Only the HVF engine ships. A saved vm.json can still name
/// a removed engine ("fast-vz" for Apple VZ, "qemu-compat" for QEMU) or one this
/// build does not know; such a VM stays listed as `.unsupported`, and its files are
/// left exactly as they are.
enum BackendKind: String {
    case hvfEngine = "hvf-engine"
    case unsupported

    init(savedKind: String) { self = BackendKind(rawValue: savedKind) == .hvfEngine ? .hvfEngine : .unsupported }

    var shortLabel: String { self == .hvfEngine ? "BridgeVM" : "Unsupported" }
    var detailLabel: String { self == .hvfEngine ? "BridgeVM engine" : "Unsupported engine" }

    /// Shown for a VM saved with a removed engine.
    static let unsupportedNotice = "This VM uses an unsupported engine. BridgeVM can’t start it. Its files remain on disk."
}
