import Foundation
import Combine

/// Top-level model: the VM library + selection. Owns one per-VM ControlModel
/// each (cached), so every VM polls and is controlled independently.
@MainActor
final class LibraryModel: ObservableObject {
    @Published var vms: [VMConfig] = []
    @Published var selectedID: String?
    @Published var showingCreate = false
    @Published var proMode = false

    private var modelCache: [String: ControlModel] = [:]

    // Host-capacity accounting (sum of RUNNING VMs vs host totals) to warn on oversubscription.
    var hostMemGiB: Double { Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824.0 }
    var hostCPU: Int { ProcessInfo.processInfo.activeProcessorCount }
    func runningModels() -> [ControlModel] { vms.compactMap { modelCache[$0.slug] }.filter { $0.running } }
    var usedMemGiB: Double { runningModels().reduce(0.0) { $0 + $1.memGiB } }
    var usedCPU: Int { runningModels().reduce(0) { $0 + $1.cpu } }

    init() {
        VMLibrary.migrateLegacyIfNeeded()
        reload()
        if selectedID == nil { selectedID = vms.first?.slug }
    }

    func reload() {
        vms = VMLibrary.list()
        let slugs = Set(vms.map { $0.slug })
        modelCache = modelCache.filter { slugs.contains($0.key) }
        if let sel = selectedID, !slugs.contains(sel) { selectedID = vms.first?.slug }
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

    func delete(_ cfg: VMConfig) {
        modelCache[cfg.slug]?.backend.stop()
        modelCache[cfg.slug] = nil
        VMLibrary.delete(cfg.slug)
        reload()
    }

    /// Add an already-built VMConfig to the library (used by the create flow).
    func add(_ cfg: VMConfig) {
        VMLibrary.save(cfg)
        reload()
        selectedID = cfg.slug
    }
}
