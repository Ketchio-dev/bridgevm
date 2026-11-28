import Foundation

struct HvfRuntimeStopPresentation: Equatable {
    let isEnabled: Bool
    let guidance: String

    static func make(runtimeActive: Bool, lifecycleBusy: Bool, ownsRuntime: Bool) -> Self {
        if lifecycleBusy {
            return Self(isEnabled: false, guidance: "Wait for the current operation to finish before shutting down the VM.")
        }
        if runtimeActive, !ownsRuntime {
            return Self(isEnabled: false, guidance: "Stop the VM from the app or tool that started it.")
        }
        if !runtimeActive {
            return Self(isEnabled: false, guidance: "The VM is stopped.")
        }
        return Self(isEnabled: true, guidance: "Shut down the VM started by this app.")
    }
}
