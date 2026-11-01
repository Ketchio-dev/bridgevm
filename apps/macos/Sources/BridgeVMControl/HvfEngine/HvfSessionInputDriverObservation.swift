import Foundation

extension HvfSessionInputDriver {
    /// The router refuses changed ownership after discarding its queue. That path returns no stream
    /// update; expose only the discarded count to diagnostics without changing routing or polling.
    func observeBindingDiscard(held: Int) {
        if router.failure == .bindingChanged, held > router.count {
            onUpdate?(.cancelled(.sessionChanged, discarded: held - router.count))
        }
    }
}
