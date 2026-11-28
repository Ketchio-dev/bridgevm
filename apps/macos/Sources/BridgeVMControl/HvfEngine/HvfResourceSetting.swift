import SwiftUI

/// RAM and CPUs. A library VM's are saved in vm.json (`memMiB`, `cpuCount`) like its other runtime-view
/// preferences, so the dashboard's start and every other start path use them. Memory goes as high as
/// Create offers on this Mac, or the saved value if that is higher; CPUs stay within VMResourceLimits.
/// The experimental entry keeps them view-local.
struct HvfResourceSetting: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var ramMiB: Int
    @Binding var cpus: Int
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }
    private var ramRange: ClosedRange<Int> {
        session.persistsLibraryPreferences
            ? CreateVMMemoryOptions.current.settingsRangeMiB(keeping: ramMiB) : 1024...65_536
    }
    private var cpuRange: ClosedRange<Int> {
        session.persistsLibraryPreferences
            ? VMResourceLimits.minimumCPU...VMResourceLimits.maximumCPU : HvfEngineConfig.supportedCPURange
    }

    var body: some View {
        HStack(spacing: 24) {
            Stepper("Memory: \(VMResourceLimits.memoryText(mib: ramMiB))", value: Binding(get: { ramMiB }, set: { update(memMiB: $0, cpus: cpus) }),
                    in: ramRange, step: 1024)
                .accessibilityIdentifier("bridgevm.runtime.ram")
            Stepper("CPUs: \(cpus)", value: Binding(get: { cpus }, set: { update(memMiB: ramMiB, cpus: $0) }),
                    in: cpuRange)
                .accessibilityIdentifier("bridgevm.runtime.cpu")
        }
        .disabled(isDisabled)
        .help(session.persistsLibraryPreferences ? "Changes apply the next time this VM starts." : "")
        .alert("Memory and CPUs", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("OK") { message = nil }
        } message: {
            Text(message ?? "")
        }
    }

    private func update(memMiB: Int, cpus newCPUs: Int) {
        let result = session.applyResources(memMiB: memMiB, cpus: newCPUs)
        ramMiB = result.memMiB
        cpus = result.cpus
        message = result.message
    }
}

extension HvfEngineSession {
    static func resourceLimitsMessage(memoryMiB: ClosedRange<Int>) -> String {
        "Choose \(memoryMiB.lowerBound / 1024)–\(memoryMiB.upperBound / 1024) GB of memory and \(VMResourceLimits.minimumCPU)–\(VMResourceLimits.maximumCPU) CPUs."
    }

    /// What the steppers show afterwards: vm.json's values for a library VM, plus any message. Only a
    /// value that changes is checked against the limits and written.
    func applyResources(memMiB: Int, cpus: Int,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (memMiB: Int, cpus: Int, message: String?) {
        guard persistsLibraryPreferences, let saved = config.libraryContext?.config else { return (memMiB, cpus, nil) }
        let memoryChanges = memMiB != saved.hvfRAMMiB, cpuChanges = cpus != saved.hvfCPUs
        let memoryRange = CreateVMMemoryOptions.current.settingsRangeMiB(keeping: saved.hvfRAMMiB)
        let message: String?
        if (memoryChanges && !memoryRange.contains(memMiB))
            || (cpuChanges && !(VMResourceLimits.minimumCPU...VMResourceLimits.maximumCPU).contains(cpus)) {
            message = Self.resourceLimitsMessage(memoryMiB: memoryRange)
        } else {
            message = saveLibraryPreference("Memory and CPUs", publish: publish) {
                if memoryChanges { $0.memMiB = memMiB }
                if cpuChanges { $0.cpuCount = cpus }
            }
        }
        let shown = config.libraryContext?.config ?? saved
        return (shown.hvfRAMMiB, shown.hvfCPUs, message)
    }
}
