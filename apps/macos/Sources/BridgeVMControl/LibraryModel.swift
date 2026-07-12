import Foundation
import Combine

/// Top-level model: the VM library + selection. Owns one per-VM ControlModel
/// each (cached), so every VM polls and is controlled independently.
@MainActor
final class LibraryModel: ObservableObject {
    static let hvfEngineSelectionID = "__bridgevm_hvf_engine_experimental__"

    @Published var vms: [VMConfig] = []
    @Published var selectedID: String?
    @Published var showingCreate = false
    @Published var proMode = false
    @Published var pendingDeletion: VMConfig?
    @Published var deletionError: String?
    @Published private(set) var deletingSlugs: Set<String> = []
    @Published private(set) var libraryIssues: [VMLibraryIssue] = []

    private var modelCache: [String: ControlModel] = [:]
    private let libraryRoot: URL

    // Host-capacity accounting (sum of RUNNING VMs vs host totals) to warn on oversubscription.
    var hostMemGiB: Double { Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824.0 }
    var hostCPU: Int { ProcessInfo.processInfo.activeProcessorCount }
    func runningModels() -> [ControlModel] { vms.compactMap { modelCache[$0.slug] }.filter { $0.running } }
    var usedMemGiB: Double { runningModels().reduce(0.0) { $0 + $1.memGiB } }
    var usedCPU: Int { runningModels().reduce(0) { $0 + $1.cpu } }

    func deletionImpact(for cfg: VMConfig) -> VMLibraryDeletionImpact {
        VMLibrary.deletionImpact(for: cfg, rootURL: libraryRoot)
    }

    init(rootURL: URL = VMLibrary.root, migrateLegacy: Bool = true) {
        libraryRoot = rootURL
        if migrateLegacy {
            VMLibrary.migrateLegacyIfNeeded(rootURL: rootURL, legacy: VMConfig.loadLegacy())
        }
        reload()
        if selectedID == nil { selectedID = vms.first?.slug }
    }

    func reload() {
        let scan = VMLibrary.scan(rootURL: libraryRoot)
        vms = scan.configs
        libraryIssues = scan.issues
        let slugs = Set(vms.map { $0.slug })
        modelCache = modelCache.filter { slugs.contains($0.key) }
        if let sel = selectedID, sel != Self.hvfEngineSelectionID, !slugs.contains(sel) { selectedID = vms.first?.slug }
    }

    func model(for cfg: VMConfig) -> ControlModel {
        if let m = modelCache[cfg.slug] { return m }
        let m = ControlModel(config: cfg)
        modelCache[cfg.slug] = m
        return m
    }

    var selectedModel: ControlModel? {
        guard let id = selectedID, let cfg = vms.first(where: { $0.slug == id }) else { return nil }
        return model(for: cfg)
    }

    func requestDeletion(_ cfg: VMConfig) {
        guard !deletingSlugs.contains(cfg.slug) else { return }
        pendingDeletion = cfg
    }

    func confirmDeletion(_ cfg: VMConfig) {
        pendingDeletion = nil
        let slug = cfg.slug
        guard !deletingSlugs.contains(slug) else { return }
        deletingSlugs.insert(slug)
        let backend = modelCache[slug]?.backend ?? cfg.makeBackend()
        let libraryRoot = self.libraryRoot
        Task.detached {
            backend.stop()
            let stillRunning = backend.isRunning()
            let deleted = !stillRunning && VMLibrary.delete(slug, rootURL: libraryRoot)
            await MainActor.run {
                self.deletingSlugs.remove(slug)
                if deleted {
                    self.modelCache[slug] = nil
                    self.reload()
                } else {
                    self.deletionError = stillRunning
                        ? "\(cfg.name)을(를) 정지하지 못해 삭제하지 않았습니다."
                        : "\(cfg.name)의 라이브러리 항목을 디스크에서 삭제하지 못했습니다."
                }
            }
        }
    }

    /// Publish a VMConfig that the create transaction has already persisted.
    /// Re-saving here would move persistence outside that transaction's rollback
    /// boundary and could report a false failure after a successful creation.
    @discardableResult
    func add(_ cfg: VMConfig) -> Bool {
        reload()
        guard vms.contains(where: { $0.slug == cfg.slug && $0.bundlePath == cfg.bundlePath }) else {
            return false
        }
        selectedID = cfg.slug
        return true
    }
}
