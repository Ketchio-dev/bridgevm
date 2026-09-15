import Foundation

extension LibraryModel {
    // Host-capacity accounting (sum of RUNNING VMs vs host totals) to warn on oversubscription.
    var hostMemGiB: Double { Double(ProcessInfo.processInfo.physicalMemory) / 1_073_741_824.0 }
    var hostCPU: Int { ProcessInfo.processInfo.activeProcessorCount }
    var usedMemGiB: Double { runningModels().reduce(0.0) { $0 + $1.memGiB } }
    var usedCPU: Int { runningModels().reduce(0) { $0 + $1.cpu } }
}
