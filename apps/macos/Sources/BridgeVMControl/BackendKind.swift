/// The engine a VM runs on — the single source of truth for the kind strings and
/// their display labels (previously duplicated across views/models).
enum BackendKind: String {
    case fastVZ = "fast-vz"
    case qemuCompat = "qemu-compat"
    case hvfEngine = "hvf-engine"

    var shortLabel: String {
        switch self { case .fastVZ: return "Fast VZ"; case .qemuCompat: return "QEMU"; case .hvfEngine: return "HVF" }
    }
    var detailLabel: String {
        switch self {
        case .fastVZ: return "Fast (Apple VZ)"
        case .qemuCompat: return "Compatibility (QEMU)"
        case .hvfEngine: return "Native (HVF)"
        }
    }
    /// Whether this engine is actually selectable/usable today.
    var available: Bool { self == .fastVZ || self == .qemuCompat || self == .hvfEngine }
}
