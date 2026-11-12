import SwiftUI

/// RAM and CPUs. A library VM's are saved in vm.json (`memMiB`, `cpuCount`) like its other runtime-view
/// preferences, so the dashboard's start and every other start path use them, within the limits the
/// other engines' resource change keeps (VMResourceLimits). The experimental entry keeps them view-local.
struct HvfResourceSetting: View {
    @ObservedObject var session: HvfEngineSession
    @Binding var ramMiB: Int
    @Binding var cpus: Int
    @State private var message: String?

    var isDisabled: Bool { session.persistsLibraryPreferences && !session.libraryPreferencesEditable }
    private var ramRange: ClosedRange<Int> {
        session.persistsLibraryPreferences
            ? VMResourceLimits.minimumMemoryMiB...VMResourceLimits.maximumMemoryMiB : 1024...65_536
    }
    private var cpuRange: ClosedRange<Int> {
        session.persistsLibraryPreferences
            ? VMResourceLimits.minimumCPU...VMResourceLimits.maximumCPU : HvfEngineConfig.supportedCPURange
    }

    static func memoryText(_ mib: Int) -> String {
        mib % 1024 == 0 ? "\(mib / 1024)GB" : String(format: "%.2fGB", Double(mib) / 1024)
    }

    var body: some View {
        HStack(spacing: 24) {
            Stepper("메모리 \(Self.memoryText(ramMiB))", value: Binding(get: { ramMiB }, set: { update(memMiB: $0, cpus: cpus) }),
                    in: ramRange, step: 1024)
                .accessibilityIdentifier("bridgevm.runtime.ram")
            Stepper("CPU \(cpus)", value: Binding(get: { cpus }, set: { update(memMiB: ramMiB, cpus: $0) }),
                    in: cpuRange)
                .accessibilityIdentifier("bridgevm.runtime.cpu")
        }
        .disabled(isDisabled)
        .help(session.persistsLibraryPreferences ? "VM이 중지된 동안 변경하면 이 VM의 모든 시작에 저장됩니다." : "")
        .alert("메모리·CPU 설정", isPresented: Binding(
            get: { message != nil }, set: { if !$0 { message = nil } }
        )) {
            Button("확인") { message = nil }
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
    static let resourceLimitsMessage = "메모리는 \(VMResourceLimits.minimumMemoryMiB / 1024)-\(VMResourceLimits.maximumMemoryMiB / 1024) GiB, CPU는 \(VMResourceLimits.minimumCPU)-\(VMResourceLimits.maximumCPU)개까지 설정할 수 있습니다."

    /// What the steppers show afterwards: vm.json's values for a library VM, plus any message. Only a
    /// value that changes is checked against the limits and written.
    func applyResources(memMiB: Int, cpus: Int,
        publish: RegistrationPublish = { VMRegistrationWriter.commit($0, to: $1) }) -> (memMiB: Int, cpus: Int, message: String?) {
        guard persistsLibraryPreferences, let saved = config.libraryContext?.config else { return (memMiB, cpus, nil) }
        let memoryChanges = memMiB != saved.hvfRAMMiB, cpuChanges = cpus != saved.hvfCPUs
        let message: String?
        if (memoryChanges && !(VMResourceLimits.minimumMemoryMiB...VMResourceLimits.maximumMemoryMiB).contains(memMiB))
            || (cpuChanges && !(VMResourceLimits.minimumCPU...VMResourceLimits.maximumCPU).contains(cpus)) {
            message = Self.resourceLimitsMessage
        } else {
            message = saveLibraryPreference("메모리·CPU 설정", publish: publish) {
                if memoryChanges { $0.memMiB = memMiB }
                if cpuChanges { $0.cpuCount = cpus }
            }
        }
        let shown = config.libraryContext?.config ?? saved
        return (shown.hvfRAMMiB, shown.hvfCPUs, message)
    }
}
