import Foundation
import Combine
/// Top-level model: the VM library + selection. Owns one per-VM ControlModel
/// each (cached), so every VM polls and is controlled independently.
@MainActor
final class LibraryModel: ObservableObject {
    static let firstRunImportSelectionID = "__bridgevm_first_run_import__"
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
    @Published var operationError: String?
    @Published var firstRunImport = FirstRunImportProgress()
    @Published private(set) var deletingSlugs: Set<String> = []
    @Published private(set) var cloningSlugs: Set<String> = []
    @Published private(set) var movingSlugs: Set<String> = []
    @Published private(set) var libraryIssues: [VMLibraryIssue] = []
    private var modelCache: [String: ControlModel] = [:]
    private let libraryRoot: URL
    let e2eUnattendedPath: String?
    private let modelFactory: @MainActor (VMConfig) -> ControlModel
    private let actionScheduler: LibraryActionScheduler
    let windowsInstallSessions: HvfWindowsInstallSessionStore
    let hvfRuntimeSessions: HvfRuntimeSessionStore
    let retainedControlStore = LibraryRetainedControlStore()
    func runningModels() -> [ControlModel] { vms.compactMap { modelCache[$0.slug] }.filter { $0.running } }
    var rootURL: URL { libraryRoot }
    func hasAcceptedControlOperation(for slug: String) -> Bool { modelCache[slug]?.hasAcceptedOperation == true }
    func hasMismatchedCachedControlConfiguration(for cfg: VMConfig) -> Bool {
        modelCache[cfg.slug].map { $0.config != cfg } ?? false
    }
    func ownsControlModel(_ model: ControlModel, for config: VMConfig) -> Bool {
        modelCache[config.slug] === model && model.config == config
    }

    init(
        rootURL: URL = VMLibrary.root,
        e2eUnattendedPath: String? = nil,
        migrateLegacy: Bool = true,
        installSessionFactory: @escaping HvfWindowsInstallSessionStore.Factory = { HvfWindowsInstallSession(plan: $0) },
        runtimeSessionFactory: @escaping HvfRuntimeSessionStore.Factory = { HvfEngineSession(config: $0) },
        actionScheduler: @escaping LibraryActionScheduler = { job in _ = Task.detached(operation: job) },
        startsModelsAutomatically: Bool = true,
        modelFactory: (@MainActor (VMConfig) -> ControlModel)? = nil
    ) {
        libraryRoot = rootURL
        self.e2eUnattendedPath = e2eUnattendedPath
        self.actionScheduler = actionScheduler
        self.modelFactory = modelFactory ?? {
            ControlModel(config: $0, backend: $0.makeBackend(libraryRoot: rootURL),
                startsAutomatically: startsModelsAutomatically)
        }
        windowsInstallSessions = HvfWindowsInstallSessionStore(makeSession: installSessionFactory)
        hvfRuntimeSessions = HvfRuntimeSessionStore(makeSession: runtimeSessionFactory)
        if migrateLegacy {
            VMLibrary.migrateLegacyIfNeeded(rootURL: rootURL, legacy: VMConfig.loadLegacy())
        }
        reload()
        configureLibraryNavigation()
    }

    func reload() {
        let scan = VMLibrary.scan(rootURL: libraryRoot)
        vms = scan.configs
        libraryIssues = scan.issues
        captureRetainedControls()
        windowsInstallSessions.reconcile(with: vms)
        hvfRuntimeSessions.reconcile(with: vms)
        let configsBySlug = Dictionary(vms.map { ($0.slug, $0) }, uniquingKeysWith: { first, _ in first })
        modelCache = modelCache.filter { slug, model in
            guard let current = configsBySlug[slug] else { return false }
            if model.config == current { return true }
            // Keep the old control handle while it may still own a live process
            // or operation. Replacing it now could make that VM impossible to stop.
            return model.running || model.lifecycleBusy || model.busy
        }
        reconcileLibrarySelection(configsBySlug: configsBySlug)
    }

    func model(for cfg: VMConfig) -> ControlModel {
        if let m = modelCache[cfg.slug] { return m }
        let m = bindControlModel(modelFactory(latestConfiguration(for: cfg)))
        modelCache[cfg.slug] = m
        return m
    }

    func confirmDeletion(_ cfg: VMConfig) {
        pendingDeletion = nil
        let slug = cfg.slug
        guard admitLibraryAction(cfg, action: .deletion) else { return }
        guard !deletingSlugs.contains(slug) else { return }
        deletingSlugs.insert(slug)
        let backend = modelCache[slug]?.backend ?? cfg.makeBackend(libraryRoot: self.libraryRoot)
        let libraryRoot = self.libraryRoot
        actionScheduler {
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

    func cloneWindowsHVF(_ cfg: VMConfig, name: String) {
        pendingWindowsClone = nil
        guard admitLibraryAction(cfg, action: .clone) else { return }
        guard !cloningSlugs.contains(cfg.slug) else { return }
        let backend = modelCache[cfg.slug]?.backend ?? cfg.makeBackend(libraryRoot: self.libraryRoot)
        guard !backend.isRunning() else {
            cloneError = "실행 중인 VM은 복제할 수 없습니다. 먼저 완전히 정지하세요."
            return
        }
        cloningSlugs.insert(cfg.slug)
        let libraryRoot = self.libraryRoot
        actionScheduler {
            let clone = HvfProtectedTransfers.clone(
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
                    self.cloneError = "Windows HVF 복제 완료를 확인하지 못했습니다. 원본 사용 여부와 대상 번들 상태를 확인하세요."
                }
            }
        }
    }

    func moveWindowsHVFBundle(_ cfg: VMConfig, to destinationParent: URL) {
        guard admitLibraryAction(cfg, action: .move) else { return }
        guard !movingSlugs.contains(cfg.slug) else { return }
        let backend = modelCache[cfg.slug]?.backend ?? cfg.makeBackend(libraryRoot: self.libraryRoot)
        guard !backend.isRunning() else {
            moveError = "실행 중인 VM은 이동할 수 없습니다. 먼저 완전히 정지하세요."
            return
        }
        movingSlugs.insert(cfg.slug)
        let libraryRoot = self.libraryRoot
        actionScheduler {
            let moved = HvfProtectedTransfers.move(cfg,
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
                    self.moveError = "VM 이동 완료를 확인하지 못했습니다. 원본과 대상 위치의 번들 및 등록 상태를 확인하세요."
                }
            }
        }
    }


}
