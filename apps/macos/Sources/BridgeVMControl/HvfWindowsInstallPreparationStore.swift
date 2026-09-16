import Foundation

@MainActor
final class HvfWindowsInstallPreparationStore {
    private typealias Handle = HvfWindowsInstallPreparation
    let options: HvfWindowsInstallPreparationOptions
    private var handles: [String: Handle] = [:]

    init(options: HvfWindowsInstallPreparationOptions) { self.options = options }

    // Called from view construction: only new handles are initialized here.
    // Existing observed state is changed only by retry, reconcile, or completion.
    func lookup(slug: String, owner: LibraryModel) -> HvfWindowsInstallPreparation {
        if let entry = owner.windowsInstallSessions.record(for: slug), entry.session.isRunning {
            return ready(entry.session, slug: slug, owner: owner)
        }
        guard let input = owner.currentWindowsInstallPreparationInput(slug: slug, repoRoot: options.repoRoot) else {
            let lookup = Handle.Lookup.unavailable(owner.vms.first(where: { $0.slug == slug }))
            if let handle = handles[slug], handle.lookup == lookup { return handle }
            return insert(slug: slug, lookup: lookup, state: .failed(Handle.missingMessage), owner: owner)
        }
        if let session = owner.cachedWindowsInstallSession(for: input) {
            return ready(session, slug: slug, owner: owner)
        }
        if let handle = handles[slug], handle.lookup == .input(input) {
            switch handle.state {
            case .preparing, .failed: return handle
            case .ready: break
            }
        }
        let expected = owner.windowsInstallSessions.record(for: slug)?.session
        let handle = insert(slug: slug, lookup: .input(input), state: .preparing, owner: owner)
        schedule(handle, input: input, replacing: expected, attempt: handle.attemptID, owner: owner)
        return handle
    }

    private func ready(_ session: HvfWindowsInstallSession, slug: String, owner: LibraryModel) -> Handle {
        if let handle = handles[slug], case let .ready(current) = handle.state, current === session {
            return handle
        }
        let bound = owner.bindWindowsInstallSession(session, slug: slug)
        return insert(slug: slug, lookup: .ready(ObjectIdentifier(session)), state: .ready(bound), owner: owner)
    }

    private func insert(slug: String, lookup: Handle.Lookup, state: Handle.State, owner: LibraryModel) -> Handle {
        let handle = Handle(slug: slug, lookup: lookup, state: state)
        handle.retryAction = { [weak self, weak owner, weak handle] in
            guard let handle else { return }
            guard let self, let owner else { handle.invalidate(Handle.ownerMessage); return }
            self.retry(handle, owner: owner)
        }
        handles[slug] = handle
        return handle
    }

    private func retry(_ handle: Handle, owner: LibraryModel) {
        guard handles[handle.slug] === handle else { handle.invalidate(Handle.missingMessage); return }
        guard case .failed = handle.state else { return }
        if let entry = owner.windowsInstallSessions.record(for: handle.slug), entry.session.isRunning {
            handle.adopt(owner.bindWindowsInstallSession(entry.session, slug: handle.slug))
            return
        }
        guard let input = owner.currentWindowsInstallPreparationInput(slug: handle.slug, repoRoot: options.repoRoot) else {
            handle.invalidate(Handle.missingMessage)
            return
        }
        if let session = owner.cachedWindowsInstallSession(for: input) {
            handle.adopt(owner.bindWindowsInstallSession(session, slug: handle.slug))
            return
        }
        let attempt = handle.begin(input)
        let expected = owner.windowsInstallSessions.record(for: handle.slug)?.session
        schedule(handle, input: input, replacing: expected, attempt: attempt, owner: owner)
    }

    private func schedule(
        _ handle: Handle, input: HvfWindowsInstallPlanInput,
        replacing expected: HvfWindowsInstallSession?, attempt: UUID, owner: LibraryModel
    ) {
        let builder = options.builder
        let task = Task { [weak self, weak owner, weak handle] in
            let plan = await HvfWindowsInstallPlanWorker.build(input, using: builder)
            guard let handle, handle.attemptID == attempt else { return }
            guard let self, let owner else {
                handle.complete(.failed(Handle.ownerMessage), attempt: attempt)
                return
            }
            guard self.handles[handle.slug] === handle else {
                handle.complete(.failed(Handle.staleMessage), attempt: attempt)
                return
            }
            let state = owner.windowsInstallPreparationResult(plan, input: input, replacing: expected)
            handle.complete(state, attempt: attempt)
        }
        handle.attach(task, attempt: attempt)
    }

    func reconcile(with configs: [VMConfig]) {
        let current = Dictionary(configs.map { ($0.slug, $0) }, uniquingKeysWith: { first, _ in first })
        for (slug, handle) in handles {
            let config = current[slug]
            let eligible = config?.engineKind == .hvfEngine && config?.installPending == true
            if case let .input(input) = handle.lookup, config != input.config {
                handle.invalidate(Handle.staleMessage)
            }
            if !eligible { handles[slug] = nil }
        }
    }
}
