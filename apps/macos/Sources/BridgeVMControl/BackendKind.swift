/// The engine a VM runs on — the single source of truth for the kind strings and
/// their display labels. Only the HVF engine ships. A saved vm.json can still name
/// a removed engine ("fast-vz" for Apple VZ, "qemu-compat" for QEMU) or one this
/// build does not know; such a VM stays listed as `.unsupported`, and its files are
/// left exactly as they are.
enum BackendKind: String {
    case hvfEngine = "hvf-engine"
    case unsupported

    init(savedKind: String) { self = BackendKind(rawValue: savedKind) == .hvfEngine ? .hvfEngine : .unsupported }

    var shortLabel: String { self == .hvfEngine ? "HVF" : "지원 종료" }
    var detailLabel: String { self == .hvfEngine ? "Native (HVF)" : "지원하지 않는 엔진" }

    /// Shown for a VM saved with a removed engine.
    static let unsupportedNotice = "이 VM은 더 이상 지원하지 않는 엔진(Fast VZ 또는 QEMU)으로 만들어져 실행할 수 없으며, 파일은 그대로 남아 있습니다."
}
