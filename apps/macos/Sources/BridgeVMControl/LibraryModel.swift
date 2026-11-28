import Foundation
import Combine

struct VMPreparationQuit {
    let confirmed: Bool
    let onSuccess: () -> Void
}

/// Top-level model: the VM library + selection. Owns one per-VM ControlModel
/// each (cached), so every VM polls and is controlled independently.
@MainActor
final class LibraryModel: ObservableObject {
    static let firstRunImportSelectionID = "__bridgevm_first_run_import__"
    static let hvfEngineSelectionID = "__bridgevm_hvf_engine_experimental__"
    @Published var vms: [VMConfig] = []
    @Published var selectedID: String?
    @Published var proMode = false
    @Published var deletionError: String?
    @Published var deletionWarning: String?
    @Published var cloneError: String?
    @Published var moveError: String?
    @Published var operationError: String?
    @Published var showsSettingsReloadNotice = false
    @Published var firstRunImport = FirstRunImportProgress()
    @Published var preparingNewVM = false
    @Published var quitAfterVMPreparation: VMPreparationQuit?
    @Published private(set) var deletingSlugs: Set<String> = []
    @Published private(set) var cloningConfigurations: [String: VMConfig] = [:]
    @Published private(set) var movingSlugs: Set<String> = []
    @Published private(set) var libraryIssues: [VMLibraryIssue] = []
    private var modelCache: [String: ControlModel] = [:]
    private let libraryRoot: URL
    let e2eUnattendedPath: String?
    let e2eHostDiagnosticStopAdmitted: Bool
    private let modelFactory: @MainActor (VMConfig) -> ControlModel
    private let actionScheduler: LibraryActionScheduler
    private let deletionOperation: LibraryDeletionOperation
    let windowsInstallSessions: HvfWindowsInstallSessionStore
    let hvfRuntimeSessions: HvfRuntimeSessionStore
    let retainedControlStore = LibraryRetainedControlStore()
    var rootURL: URL { libraryRoot }
    var cachedControlModels: [String: ControlModel] { modelCache }

    init(
        rootURL: URL = VMLibrary.root,
        e2eUnattendedPath: String? = nil,
        e2eHostDiagnosticStopAdmitted: Bool = false,
        migrateLegacy: Bool = true,
        initialScan: VMLibraryScan? = nil,
        installSessionFactory: @escaping HvfWindowsInstallSessionStore.Factory = { HvfWindowsInstallSession(plan: $0) },
        runtimeSessionFactory: @escaping HvfRuntimeSessionStore.Factory = { HvfEngineSession(config: $0) },
        installPreparation: HvfWindowsInstallPreparationOptions = .init(),
        actionScheduler: @escaping LibraryActionScheduler = { job in _ = Task.detached(operation: job) },
        deletionOperation: @escaping LibraryDeletionOperation = LibraryDeletionKeyArchive.live,
        startsModelsAutomatically: Bool = true,
        modelFactory: (@MainActor (VMConfig) -> ControlModel)? = nil
    ) {
        libraryRoot = rootURL
        self.e2eUnattendedPath = e2eUnattendedPath
        self.e2eHostDiagnosticStopAdmitted = e2eHostDiagnosticStopAdmitted && e2eUnattendedPath != nil
        self.actionScheduler = actionScheduler; self.deletionOperation = deletionOperation
        self.modelFactory = modelFactory ?? LibraryControlModelFactory.make(
            rootURL: rootURL, startsAutomatically: startsModelsAutomatically)
        windowsInstallSessions = HvfWindowsInstallSessionStore(
            makeSession: installSessionFactory, preparation: installPreparation)
        hvfRuntimeSessions = HvfRuntimeSessionStore(makeSession: runtimeSessionFactory)
        applyScan(initialScan ?? LibraryInitialScan.perform(rootURL: rootURL, migrateLegacy: migrateLegacy))
        configureLibraryNavigation()
    }

    func reload() { applyScan(VMLibrary.scan(rootURL: libraryRoot)) }

    private func applyScan(_ scan: VMLibraryScan) {
        vms = scan.configs
        libraryIssues = scan.issues
        captureRetainedControls()
        windowsInstallSessions.reconcile(with: vms)
        hvfRuntimeSessions.reconcile(with: vms)
        let configsBySlug = Dictionary(vms.map { ($0.slug, $0) }, uniquingKeysWith: { first, _ in first })
        modelCache = LibraryControlModelReconciliation.reconcile(modelCache, configurations: configsBySlug)
        reconcileLibrarySelection(configsBySlug: configsBySlug)
    }

    func model(for cfg: VMConfig) -> ControlModel {
        if let m = modelCache[cfg.slug] { return m }
        let m = modelFactory(latestConfiguration(for: cfg))
        modelCache[cfg.slug] = m
        return m
    }

    func confirmDeletion(_ cfg: VMConfig) {
        let slug = cfg.slug
        guard admitLibraryAction(cfg, action: .deletion), !deletingSlugs.contains(slug) else { return }
        let backend = modelCache[slug]?.backend ?? cfg.makeBackend(libraryRoot: libraryRoot)
        if backend.diskHelperIsRunning() { deletionError = VMDiskHelper.busy; return }
        deletingSlugs.insert(slug)
        let root = libraryRoot, deletion = deletionOperation
        actionScheduler {
            let result = deletion(cfg, root, backend.isRunning)
            await MainActor.run {
                self.deletingSlugs.remove(slug)
                let deleted = result.outcome == .deleted
                if deleted {
                    self.modelCache[slug] = nil; self.deletionWarning = result.warning ?? self.deletionWarning
                } else {
                    self.deletionError = result.outcome == .running
                        ? "Shut down \(cfg.name) before deleting it."
                        : LibraryDeletionReferences.failureMessage(for: cfg, rootURL: root)
                }
                // A failed delete may still have removed part of the entry: list what is on disk now.
                self.reload()
                self.finishVMPreparation(succeeded: deleted)
            }
        }
    }

    func cloneWindowsHVF(_ cfg: VMConfig, name: String,
                         operation: @escaping LibraryCloneOperation = { HvfProtectedTransfers.clone(name: $0, template: $1, libraryRoot: $2) }) {
        guard admitLibraryAction(cfg, action: .clone), !cloningSlugs.contains(cfg.slug) else { return }
        let backend = modelCache[cfg.slug]?.backend ?? cfg.makeBackend(libraryRoot: self.libraryRoot)
        guard !backend.isRunning() else {
            cloneError = "Shut down the VM before duplicating it."
            return
        }
        if backend.diskHelperIsRunning() { cloneError = VMDiskHelper.busy; return }
        cloningConfigurations[cfg.slug] = cfg
        let libraryRoot = self.libraryRoot
        let selectionAtAdmission = selectedID
        actionScheduler {
            let clone = operation(name, cfg, libraryRoot)
            await MainActor.run {
                self.cloningConfigurations[cfg.slug] = nil
                if let clone {
                    self.reload(revealing: clone.slug, ifSelectionIs: selectionAtAdmission)
                } else {
                    // The copy's half-made bundle is already gone; say what the source is missing, if
                    // that is why, and show a copy whose vm.json was written but not confirmed.
                    self.cloneError = (cfg.engineKind == .hvfEngine ? HvfWindowsCloneSource.refusal(for: cfg, phase: .copy) : nil)
                        ?? "BridgeVM couldn’t duplicate “\(cfg.name)”. Check that your Mac has enough free space for a copy of its disk, then try again."
                    self.reload()
                }
                self.finishVMPreparation(succeeded: clone != nil)
            }
        }
    }

    /// The bundle keeps its folder name (every VM's is bundle.vmbridge), so a folder that already holds
    /// one, or the folder it is in, can't take it. Both used to end in "could not confirm the move".
    nonisolated static func moveDestinationRefusal(_ cfg: VMConfig, to destinationParent: URL) -> String? {
        let source = URL(fileURLWithPath: cfg.bundlePath, isDirectory: true).standardizedFileURL
        let parent = destinationParent.standardizedFileURL
        if parent.resolvingSymlinksInPath() == source.deletingLastPathComponent().resolvingSymlinksInPath() {
            return "This VM is already in that folder. Choose another one."
        }
        guard FileManager.default.fileExists(atPath: parent.appendingPathComponent(source.lastPathComponent).path) else { return nil }
        return "“\(parent.lastPathComponent)” already holds a VM bundle. Choose another folder, or make a new folder for this VM."
    }

    func moveWindowsHVFBundle(_ cfg: VMConfig, to destinationParent: URL) {
        guard admitLibraryAction(cfg, action: .move) else { return }
        guard !movingSlugs.contains(cfg.slug) else { return }
        let backend = modelCache[cfg.slug]?.backend ?? cfg.makeBackend(libraryRoot: self.libraryRoot)
        guard !backend.isRunning() else {
            moveError = "Shut down the VM before moving it."
            return
        }
        if backend.diskHelperIsRunning() { moveError = VMDiskHelper.busy; return }
        if let refusal = Self.moveDestinationRefusal(cfg, to: destinationParent) {
            moveError = refusal
            return
        }
        movingSlugs.insert(cfg.slug)
        let libraryRoot = self.libraryRoot
        let selectionAtAdmission = selectedID
        actionScheduler {
            let moved = HvfProtectedTransfers.move(cfg,
                to: destinationParent,
                rootURL: libraryRoot
            )
            await MainActor.run {
                self.movingSlugs.remove(cfg.slug)
                if let moved {
                    self.modelCache[cfg.slug] = nil
                    self.reload(revealing: moved.slug, ifSelectionIs: selectionAtAdmission)
                } else {
                    // A drive pulled after the folder was chosen. A bundle that left but could not be
                    // committed or put back keeps its unfinished-move record, and the old sentence.
                    let pending = VMRelocationJournal.isPending(cfg, rootURL: self.libraryRoot)
                    self.moveError = (pending ? nil : Self.missingBundleRefusal(cfg))
                        ?? VMRelocationReferences.failureMessage(for: cfg, rootURL: self.libraryRoot)
                    self.reload() // It may have committed: replace stale idle handles with the persisted registration.
                }
                self.finishVMPreparation(succeeded: moved != nil)
            }
        }
    }
}

extension LibraryModel {
    static let missingBundleMessage = "BridgeVM can’t find this VM’s files. Connect the drive that holds them, then try again."

    /// The saved bundle is gone (its drive is disconnected, say). Move used to ask for a folder and then
    /// fail with “could not confirm the move”, and Show in Finder opened nothing.
    static func missingBundleRefusal(_ cfg: VMConfig) -> String? {
        (try? cfg.bundleIsAbsent()) == true ? missingBundleMessage : nil
    }

    /// Checked before the folder panel opens.
    func admitBundleMove(_ cfg: VMConfig) -> Bool {
        guard let missing = Self.missingBundleRefusal(cfg) else { return true }
        moveError = missing
        return false
    }

    func revealBundleInFinder(_ cfg: VMConfig, reveal: ([URL]) -> Void) {
        if let missing = Self.missingBundleRefusal(cfg) { operationError = missing; return }
        reveal([URL(fileURLWithPath: cfg.bundlePath)])
    }
}

extension LibraryModel {
    func reload(revealing resultSlug: String, ifSelectionIs selectionAtAdmission: String?) {
        // Compare current identity before reconciliation, not navigation history.
        let revealResult = selectedID == selectionAtAdmission
        reload()
        if revealResult { selectedID = resultSlug }
    }
}

extension LibraryModel {
    var hostMemGiB: Double { Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824.0 }
    var hostCPU: Int { ProcessInfo.processInfo.activeProcessorCount }
}

extension LibraryModel {
    @discardableResult
    func returnToFirstRunInputs() -> Bool {
        guard !firstRunImportBusy, firstRunImport.publishedConfig != nil else { return false }
        firstRunImport.returnToInputs()
        proMode = false
        selectedID = LibraryModel.firstRunImportSelectionID
        return true
    }

    /// Drops a finished import's failure so Get Started and the next form start clean. A running
    /// import and a saved VM that still has to be opened keep their status.
    @discardableResult
    func discardFirstRunImportFailure() -> Bool {
        guard !firstRunImportBusy, firstRunImport.publishedConfig == nil else { return false }
        if firstRunImport.error != nil { firstRunImport.returnToInputs() }
        return true
    }
}

extension LibraryModel {
    func requestDeletion(_ cfg: VMConfig) -> VMConfig? {
        guard admitLibraryAction(cfg, action: .deletion), !deletingSlugs.contains(cfg.slug) else { return nil }
        return cfg
    }

    func requestWindowsClone(_ cfg: VMConfig) -> VMConfig? {
        guard cfg.engineKind == .hvfEngine,
              cfg.installPending != true,
              admitLibraryAction(cfg, action: .clone), !cloningSlugs.contains(cfg.slug) else { return nil }
        return cfg
    }
}

extension LibraryModel {
    func admitLibraryAction(_ cfg: VMConfig, action: LibraryFileAction) -> Bool {
        let source = cloneSourceRefusal(for: cfg, action: action)
        let pending = action == .deletion ? pendingDeletionReferenceRefusal(for: cfg) : transactionResidueRefusal(for: cfg, action: action)
        guard let refusal = libraryActionRefusal(for: cfg) ?? source ?? pending else { return true }
        switch action {
        case .deletion: deletionError = refusal
        case .clone: cloneError = refusal
        case .move: moveError = refusal
        }
        return false
    }
}

extension LibraryModel {
    func hasMismatchedCachedControlConfiguration(for cfg: VMConfig) -> Bool {
        cachedControlModels[cfg.slug].map { $0.config != cfg } ?? false
    }
}

extension LibraryModel {
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

extension LibraryModel {
    var selectedModel: ControlModel? {
        guard let id = selectedID, let cfg = vms.first(where: { $0.slug == id }) else { return nil }
        return model(for: cfg)
    }

    var selectedDetail: (config: VMConfig, model: ControlModel)? {
        guard let id = selectedID, let latest = vms.first(where: { $0.slug == id }) else { return nil }
        let model = model(for: latest)
        // Keep an active control handle's accepted configuration reachable.
        // Once idle, use current registration even before another cache reload.
        let config = model.running ? model.config : latest
        return (config, model)
    }
}

extension LibraryModel {
    /// A fresh stop ID may join a retained record only while its run is still the VM's current owned state.
    /// A pending start, live replacement or attachment means an older completed run must not answer for the VM.
    func retainsOwnedStopTarget(_ request: NativeRuntimeControlRequest) -> Bool {
        guard let token = UUID(uuidString: request.target.runToken) else { return false }
        let records = runtimeRecords(slug: request.vmID)
        let identity = HvfOwnedRuntimeIdentity(token: token, processID: request.target.processID)
        let sessions = records.map { ($0.session.runtimeObservation(), $0.session.hasPendingRuntimeStart) }
        if sessions.contains(where: { $0.0.ownedProcessIdentity == identity }) { return true }
        guard !sessions.contains(where: { $0.1 || $0.0.ownedProcessIdentity != nil || $0.0.hasRetainedAttachment }) else {
            return false
        }
        return sessions.contains { $0.0.lastOwnedExit?.identity == identity }
    }
}

extension LibraryModel {
    func requestOwnedRuntimeStop(_ request: NativeRuntimeControlRequest) throws -> HvfOwnedStopAdmission {
        guard NativeLibraryReader.isCanonicalID(request.vmID), let token = UUID(uuidString: request.target.runToken),
              let operationID = UUID(uuidString: request.operationID) else { throw NativeRuntimeError.invalidMessage }
        let identity = HvfOwnedRuntimeIdentity(token: token, processID: request.target.processID)
        let matches = runtimeRecords(slug: request.vmID).filter {
            $0.session.runtimeObservation().ownedProcessIdentity == identity
        }
        guard !matches.isEmpty else { throw NativeRuntimeStopRefused.targetUnavailable }
        guard matches.count == 1, let record = matches.first else { throw NativeRuntimeStopRefused.ambiguousTarget }
        return record.session.requestOwnedStop(target: identity, operationID: operationID)
    }
}

extension LibraryModel {
    @discardableResult
    func dismissRetainedControl(_ token: String) -> Bool {
        guard retainedControlStore.dismiss(token) else { return false }
        if selectedID == token { selectedID = vms.first?.slug }
        return true
    }
}

typealias LibraryCloneOperation = @Sendable (String, VMConfig, URL) -> VMConfig?

extension LibraryModel {
    var cloningSlugs: Set<String> { Set(cloningConfigurations.keys) }

    /// What the row and the dashboard say while a delete, move or duplicate of this VM runs: a large
    /// copy takes minutes, and without it the VM read "Stopped" until Start was refused.
    func fileOperationTitle(for slug: String) -> String? {
        if deletingSlugs.contains(slug) { return "Deleting…" }
        if movingSlugs.contains(slug) { return "Moving…" }
        if cloningSlugs.contains(slug) { return "Duplicating…" }
        return nil
    }
}

extension LibraryModel {
    /// Accepted plans can outlive their registration or a subsequently edited request.
    func pendingDeletionReferenceRefusal(for config: VMConfig) -> String? {
        guard config.engineKind == .hvfEngine, config.installPending == true else { return nil }
        let entry = rootURL.appendingPathComponent(config.slug)
        let controls = windowsInstallSessions.retainedControls(excludingSlugs: [])
            + hvfRuntimeSessions.retainedControls(excludingSlugs: [])
            + retainedControlStore.records.map(\.descriptor)
        for control in controls where control.isActive {
            let request: HvfWindowsInstallRequest?
            switch control {
            case let .install(_, session): request = session.plan.request
            case .runtime: request = nil
            }
            if HvfPendingDeletionReferences.references(config: control.config,
                                                        request: request, removedEntry: entry) {
                return "Another VM is using these files to run or install Windows. Finish or cancel that operation and try again."
            }
        }
        return nil
    }
}

extension LibraryModel {
    func bindRuntimeSession(_ session: HvfEngineSession, configuration: VMConfig) {
        session.library = self
        bindRuntimeWorkAdmission(session, configuration: configuration)
        session.onConfigurationPersisted = { [weak self, weak session] previous, saved in
            guard let self, let session else { return }
            self.adoptSavedRuntimePreferences(session, previous: previous, saved: saved)
        }
    }

    /// Rebind the stopped session after its settings are saved so its open views keep their pending edits.
    private func adoptSavedRuntimePreferences(_ session: HvfEngineSession, previous: VMConfig, saved: VMConfig) {
        var expected = saved.keepingRuntimePreferences(of: previous)
        expected.id = previous.id
        if expected == previous, let context = session.config.libraryContext,
           let launch = HvfEngineConfig.libraryVM(saved, rootURL: context.rootURL,
               e2eHostDiagnosticStopAdmitted: context.e2eHostDiagnosticStopAdmitted),
           hvfRuntimeSessions.rebind(session, from: previous, to: saved) {
            session.config = session.config.adoptingSavedPreferences(launch, previous: previous, saved: saved)
        }
        reload()
        if hvfRuntimeSession(for: saved) !== session {
            showsSettingsReloadNotice = true
        }
    }
}

extension LibraryModel {
    /// The VM's saved configuration as read from disk now; it must be the loaded library entry.
    func validatedInstallConfiguration(vmID: String, library: NativeRuntimeLibraryHandle) throws -> VMConfig {
        try library.validateCurrentIdentity()
        let matches = vms.filter { $0.slug == vmID }
        guard !matches.isEmpty else { throw NativeInstallControlRefusal.targetUnavailable }
        guard matches.count == 1, let current = matches.first else {
            throw NativeInstallControlRefusal.ambiguousTarget
        }
        guard let saved = try? NativeCLIStatusConfigReader.read(library: library, id: vmID), current == saved else {
            throw NativeInstallControlRefusal.configurationChanged
        }
        guard saved.engineKind == .hvfEngine, saved.installPending == true else {
            throw NativeInstallControlRefusal.unsupportedTarget
        }
        try library.validateCurrentIdentity()
        return saved
    }
}

extension LibraryModel {
    func captureRetainedControls() {
        let registered = Set(vms.map(\.slug))
        retainedControlStore.capture(
            windowsInstallSessions.retainedControls(excludingSlugs: registered)
                + hvfRuntimeSessions.retainedControls(excludingSlugs: registered))
    }
}

@MainActor
enum LibraryControlModelReconciliation {
    static func reconcile(_ cache: [String: ControlModel], configurations: [String: VMConfig]) -> [String: ControlModel] {
        cache.filter { slug, model in
            guard let current = configurations[slug] else { return false }
            // A running VM keeps the model it was observed with until it stops.
            return model.config == current || model.running
        }
    }
}

typealias LibraryWorkAdmission = @MainActor (_ reportRefusal: Bool) -> String?

extension LibraryModel {
    func deletionImpact(for cfg: VMConfig) -> VMLibraryDeletionImpact {
        VMLibrary.deletionImpact(for: cfg, rootURL: rootURL)
    }

}

extension LibraryModel {
    func latestConfiguration(for config: VMConfig) -> VMConfig {
        vms.first(where: { $0.slug == config.slug }) ?? config
    }

    static let unavailableMessage = "The VM library is unavailable. Return to the library and try again."

    func boundWorkAdmission(
        slug: String,
        owns: @escaping @MainActor (LibraryModel, VMConfig) -> Bool
    ) -> LibraryWorkAdmission {
        { [weak self] reportRefusal in
            guard let self else {
                return LibraryModel.unavailableMessage
            }
            let refusal = self.libraryWorkRefusal(slug: slug, owns: owns)
            if reportRefusal, let refusal { self.operationError = refusal }
            return refusal
        }
    }

}

extension LibraryModel {
    /// `bridgevm app start`: the library's Start button on the VM's current saved configuration.
    func requestCLIStart(vmID: String) -> NativeRuntimeStartAdmission {
        // The CLI or another tool may have saved vm.json after this app last read it, or published a
        // new VM: read the library once, as the manual refresh does, and start what is saved.
        let current = vms.first { $0.slug == vmID }
        if current == nil || (try? NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false))
            .flatMap({ try? NativeCLIStatusConfigReader.read(library: $0, id: vmID) }) != current { reload() }
        guard let saved = vms.first(where: { $0.slug == vmID }) else {
            return .refused("No VM has the ID ‘\(vmID)’ in this library. Run `bridgevm app list` to find its ID.")
        }
        guard saved.engineKind == .hvfEngine else {
            return .refused("The start command supports Windows VMs that use the BridgeVM engine.")
        }
        guard saved.installPending != true else {
            return .refused("Finish installing Windows in BridgeVM before starting this VM.")
        }
        guard let launch = HvfEngineConfig.libraryVM(saved, rootURL: rootURL,
                  e2eHostDiagnosticStopAdmitted: e2eHostDiagnosticStopAdmitted),
              let session = hvfRuntimeDetailSession(for: saved) else {
            return .refused("BridgeVM could not read the saved VM configuration. Reload the library and try again.")
        }
        // No microphone prompt: nobody may be at the Mac to answer it, and the start would wait.
        switch session.requestGUIStart(configuration: launch, policy: .requireNew, microphone: nil) {
        case let .accepted(ticket): return .accepted(ticket, session)
        case let .refused(reason): return .refused(reason)
        }
    }
}

extension LibraryModel {
    func bindRuntimeWorkAdmission(_ session: HvfEngineSession, configuration: VMConfig) {
        let owns: @MainActor (LibraryModel, VMConfig) -> Bool = { [weak session] owner, current in
            guard let session else { return false }
            return current.engineKind == .hvfEngine && current.installPending != true
                && owner.hvfRuntimeSessions.owns(session, for: current)
        }
        session.workAdmission = boundWorkAdmission(slug: configuration.slug, owns: owns)
        session.reservedWorkAdmission = { [weak self, weak session] token, reportRefusal in
            guard let self, let session, session.matchesRuntimeReservation(token) else {
                return "The operation on this VM changed. Check its status and try again."
            }
            let refusal = self.libraryWorkRefusal(slug: configuration.slug,
                excluding: (session, token), owns: owns)
            if reportRefusal, let refusal { self.operationError = refusal }
            return refusal
        }
    }

    func runtimeWorkConflicts(slug: String, excluding reservation: (HvfEngineSession, UUID)?) -> Bool {
        runtimeRecords(slug: slug).contains { record in
            if let (session, token) = reservation, record.session === session,
               session.matchesRuntimeReservation(token) { return false }
            return record.session.hasActiveRuntimeWork
        }
    }

}

extension LibraryModel {
    func selectedPaletteCommands(dismiss: @escaping () -> Void) -> [PaletteCommand] {
        if selectedID == Self.hvfEngineSelectionID {
            return [paletteControlNavigation(id: Self.hvfEngineSelectionID, name: "HVF Engine", dismiss: dismiss)]
        }
        guard let id = selectedID, let config = vms.first(where: { $0.slug == id }) else { return [] }
        return [paletteControlNavigation(id: id, name: config.name, dismiss: dismiss)]
    }

    private func paletteControlNavigation(id: String, name: String, dismiss: @escaping () -> Void) -> PaletteCommand {
        .init(id: "open.\(id)", title: "Open controls for \(name)", subtitle: "View startup, shutdown and installation controls", systemImage: "desktopcomputer") {
            self.selectedID = id
            self.proMode = false
            dismiss()
        }
    }
}

extension LibraryModel {
    var retainedControlRecords: [LibraryRetainedControlRecord] { retainedControlStore.records }

    var selectedRetainedControl: LibraryRetainedControlRecord? {
        retainedControlRecords.first { $0.id == selectedID }
    }

    func configureLibraryNavigation() {
        retainedControlStore.onChange = { [weak self] in self?.objectWillChange.send() }
        if selectedID == nil { selectedID = vms.first?.slug }
    }

    func reconcileLibrarySelection(configsBySlug: [String: VMConfig]) {
        if selectedRetainedControl != nil { return }
        if let sel = selectedID,
           ![Self.hvfEngineSelectionID, Self.firstRunImportSelectionID].contains(sel),
           configsBySlug[sel] == nil, let record = activeRetainedControl(for: sel) {
            selectedID = record.id
            return
        }
        if let sel = selectedID, ![Self.hvfEngineSelectionID, Self.firstRunImportSelectionID].contains(sel), configsBySlug[sel] == nil {
            selectedID = vms.first?.slug
        }
    }

    private func activeRetainedControl(for slug: String) -> LibraryRetainedControlRecord? {
        retainedControlRecords
            .filter { $0.descriptor.config.slug == slug && $0.descriptor.isActive }
            .min { $0.descriptor.sortKey.lexicographicallyPrecedes($1.descriptor.sortKey) }
    }
}

extension LibraryModel {
    /// Every runtime session the app holds, retained ones included, each once.
    func appRuntimeSessions() -> [HvfEngineSession] {
        var seen: Set<ObjectIdentifier> = []
        let retained = retainedControlStore.records.compactMap { record -> HvfEngineSession? in
            guard case let .runtime(_, session) = record.descriptor else { return nil }
            return session
        }
        return (hvfRuntimeSessions.sessions + retained).filter { seen.insert(ObjectIdentifier($0)).inserted }
    }

    func appTerminationSessions() -> [ControlAppTerminationSession] {
        let retained = retainedControlStore.records.compactMap { record -> HvfWindowsInstallSession? in
            guard case let .install(_, session) = record.descriptor else { return nil }
            return session
        }
        var seen: Set<ObjectIdentifier> = []
        let installs = (windowsInstallSessions.sessions + retained).filter { seen.insert(ObjectIdentifier($0)).inserted }
        return appRuntimeSessions().map { $0 as ControlAppTerminationSession }
            + installs.map { $0 as ControlAppTerminationSession }
    }

    /// Existing handles only: no session factory, process scan, or guest effect.
    /// The wire's session cap applies only to the status response (runtimeObservations).
    func runtimeRecords(slug: String) -> [HvfRuntimeSessionRecord] {
        var seen: Set<ObjectIdentifier> = []
        let retained = retainedControlStore.records.compactMap { record -> HvfRuntimeSessionRecord? in
            guard case let .runtime(config, session) = record.descriptor, config.slug == slug else { return nil }
            return HvfRuntimeSessionRecord(sourceConfig: config, session: session)
        }
        return ([hvfRuntimeSessions.existingRecord(slug: slug)].compactMap { $0 } + retained)
            .filter { seen.insert(ObjectIdentifier($0.session)).inserted }
    }
}

extension LibraryModel {
    func beginOwnedInstallPreparation(saved: VMConfig,
        library: NativeRuntimeLibraryHandle, operation: NativeInstallOperation,
        validateOwner: @escaping () throws -> Void
    ) {
        let options = windowsInstallSessions.preparation.options
        guard let input = currentWindowsInstallPreparationInput(slug: saved.slug,
            repoRoot: options.repoRoot), input.config == saved else {
            operation.fail(HvfWindowsInstallPreparation.staleMessage); return
        }
        let expected = windowsInstallSessions.record(for: saved.slug)?.session
        let task = Task { [weak self, weak operation] in
            let plan = await HvfWindowsInstallPlanWorker.build(input, using: options.builder)
            guard !Task.isCancelled, let self, let operation else { return }
            finishOwnedInstallPreparation(plan, input: input, expected: expected,
                saved: saved, library: library, operation: operation, validateOwner: validateOwner)
        }
        operation.attach(task)
    }

    private func finishOwnedInstallPreparation(_ plan: HvfWindowsInstallPlan,
        input: HvfWindowsInstallPlanInput, expected: HvfWindowsInstallSession?,
        saved: VMConfig, library: NativeRuntimeLibraryHandle,
        operation: NativeInstallOperation, validateOwner: () throws -> Void
    ) {
        guard case let .success(current) = windowsInstallSessions.cliOperations.operation(vmID: saved.slug),
              current === operation, operation.reservesWork else { return }
        do {
            try validateOwner()
            // The plan was built from `saved`: settings saved meanwhile must not run an install from stale inputs.
            guard try validatedInstallConfiguration(vmID: saved.slug, library: library) == saved else {
                throw NativeInstallControlRefusal.configurationChanged
            }
        } catch {
            operation.fail(HvfWindowsInstallPreparation.staleMessage); return
        }
        switch windowsInstallPreparationResult(plan, input: input, replacing: expected) {
        case let .failed(message): operation.fail(message)
        case .preparing: operation.fail(HvfWindowsInstallPreparation.staleMessage)
        case let .ready(session):
            _ = bindWindowsInstallSession(session, slug: saved.slug)
            guard operation.start(session) else {
                operation.fail("Windows installation couldn’t start. Check the VM in the library and try again."); return
            }
        }
    }

}

extension LibraryModel {
    func requestOwnedInstall(_ request: NativeInstallControlRequest,
                             validateOwner: @escaping () throws -> Void) throws -> NativeInstallModelResult {
        try NativeInstallControlCodec.validate(request)
        guard NativeLibraryReader.isCanonicalID(request.vmID) else { throw NativeRuntimeError.invalidMessage }
        try validateOwner()
        let library = try NativeRuntimeLibraryHandle.open(rootURL: rootURL, create: false)
        guard library.identity == request.library else { throw NativeInstallControlRefusal.configurationChanged }
        // The CLI or another tool may have saved vm.json after this app last read it, or published a
        // new VM: read the library once, as the manual refresh does, and use what is saved.
        if vms.first(where: { $0.slug == request.vmID })
            != (try? NativeCLIStatusConfigReader.read(library: library, id: request.vmID)) { reload() }
        let store = windowsInstallSessions.cliOperations
        if let session = windowsInstallSessions.record(for: request.vmID)?.session {
            store.adopt(vmID: request.vmID, session: session)
        }
        switch request.operation {
        case .install:
            // Status and cancel name an operation this app already holds: a finished install has saved
            // installPending = false, which only a new install must refuse.
            let saved = try validatedInstallConfiguration(vmID: request.vmID, library: library)
            guard let id = request.operationID.flatMap(UUID.init(uuidString:)) else {
                throw NativeRuntimeError.invalidMessage
            }
            switch store.reserve(vmID: saved.slug, operationID: id) {
            case let .accepted(operation):
                guard libraryWorkRefusal(slug: saved.slug, excludingInstall: operation,
                    owns: { _, current in current == saved }) == nil else {
                    operation.fail("Another operation is using this VM. Try again when it finishes.")
                    throw NativeInstallControlRefusal.busy
                }
                beginOwnedInstallPreparation(saved: saved, library: library,
                    operation: operation, validateOwner: validateOwner)
                return .init(disposition: .accepted, operation: operation)
            case let .existing(operation): return .init(disposition: .existing, operation: operation)
            case let .refused(refusal): throw refusal
            }
        case .installStatus:
            return try existingInstallResult(store, request: request, disposition: .status)
        case .installCancel:
            let result = try existingInstallResult(store, request: request, disposition: .cancelAccepted)
            guard result.operation.cancel() else { throw NativeInstallControlRefusal.cancellationUnavailable }
            return result
        }
    }

    private func existingInstallResult(_ store: NativeInstallOperationStore,
        request: NativeInstallControlRequest, disposition: NativeInstallControlResponse.Disposition
    ) throws -> NativeInstallModelResult {
        switch store.operation(vmID: request.vmID) {
        case let .success(operation): return .init(disposition: disposition, operation: operation)
        case let .failure(refusal): throw refusal
        }
    }
}
