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
    @Published var pendingWindowsClone: VMConfig?
    @Published var deletionError: String?
    @Published var cloneError: String?
    @Published var moveError: String?
    @Published private(set) var deletingSlugs: Set<String> = []
    @Published private(set) var cloningSlugs: Set<String> = []
    @Published private(set) var movingSlugs: Set<String> = []
    @Published private(set) var libraryIssues: [VMLibraryIssue] = []

    private var modelCache: [String: ControlModel] = [:]
    private let libraryRoot: URL
    private let modelFactory: @MainActor (VMConfig) -> ControlModel

    // Host-capacity accounting (sum of RUNNING VMs vs host totals) to warn on oversubscription.
    var hostMemGiB: Double { Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824.0 }
    var hostCPU: Int { ProcessInfo.processInfo.activeProcessorCount }
    func runningModels() -> [ControlModel] { vms.compactMap { modelCache[$0.slug] }.filter { $0.running } }
    var usedMemGiB: Double { runningModels().reduce(0.0) { $0 + $1.memGiB } }
    var usedCPU: Int { runningModels().reduce(0) { $0 + $1.cpu } }

    func deletionImpact(for cfg: VMConfig) -> VMLibraryDeletionImpact {
        VMLibrary.deletionImpact(for: cfg, rootURL: libraryRoot)
    }

    init(
        rootURL: URL = VMLibrary.root,
        migrateLegacy: Bool = true,
        modelFactory: @escaping @MainActor (VMConfig) -> ControlModel = { ControlModel(config: $0) }
    ) {
        libraryRoot = rootURL
        self.modelFactory = modelFactory
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
        let configsBySlug = Dictionary(vms.map { ($0.slug, $0) }, uniquingKeysWith: { first, _ in first })
        modelCache = modelCache.filter { slug, model in
            guard let current = configsBySlug[slug] else { return false }
            if model.config == current { return true }
            // Keep the old control handle while it may still own a live process
            // or operation. Replacing it now could make that VM impossible to stop.
            return model.running || model.lifecycleBusy || model.busy
        }
        if let sel = selectedID, sel != Self.hvfEngineSelectionID, configsBySlug[sel] == nil {
            selectedID = vms.first?.slug
        }
    }

    func model(for cfg: VMConfig) -> ControlModel {
        if let m = modelCache[cfg.slug] { return m }
        let m = modelFactory(cfg)
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

    func requestWindowsClone(_ cfg: VMConfig) {
        guard cfg.engineKind == .hvfEngine,
              cfg.installPending != true,
              !cloningSlugs.contains(cfg.slug) else { return }
        pendingWindowsClone = cfg
    }

    func cloneWindowsHVF(_ cfg: VMConfig, name: String) {
        pendingWindowsClone = nil
        guard !cloningSlugs.contains(cfg.slug) else { return }
        let backend = modelCache[cfg.slug]?.backend ?? cfg.makeBackend()
        guard !backend.isRunning() else {
            cloneError = "실행 중인 VM은 복제할 수 없습니다. 먼저 완전히 정지하세요."
            return
        }
        cloningSlugs.insert(cfg.slug)
        let libraryRoot = self.libraryRoot
        Task.detached {
            let clone = VMLibrary.cloneWindowsHVF(
                name: name,
                template: cfg,
                libraryRoot: libraryRoot
            )
            await MainActor.run {
                self.cloningSlugs.remove(cfg.slug)
                if let clone {
                    self.reload()
                    self.selectedID = clone.slug
                } else {
                    self.cloneError = "Windows HVF 번들을 복제하지 못했습니다. 원본 데이터는 변경하지 않았습니다."
                }
            }
        }
    }

    func moveWindowsHVFBundle(_ cfg: VMConfig, to destinationParent: URL) {
        guard !movingSlugs.contains(cfg.slug) else { return }
        let backend = modelCache[cfg.slug]?.backend ?? cfg.makeBackend()
        guard !backend.isRunning() else {
            moveError = "실행 중인 VM은 이동할 수 없습니다. 먼저 완전히 정지하세요."
            return
        }
        movingSlugs.insert(cfg.slug)
        let libraryRoot = self.libraryRoot
        Task.detached {
            let moved = VMLibrary.moveWindowsHVFBundle(
                cfg,
                to: destinationParent,
                rootURL: libraryRoot
            )
            await MainActor.run {
                self.movingSlugs.remove(cfg.slug)
                if let moved {
                    self.modelCache[cfg.slug] = nil
                    self.reload()
                    self.selectedID = moved.slug
                } else {
                    self.moveError = "VM 번들을 이동하지 못했습니다. 원래 위치와 등록 정보를 복구했습니다. 대상 위치가 비어 있고 같은 Mac에서 쓸 수 있는지 확인하세요."
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
