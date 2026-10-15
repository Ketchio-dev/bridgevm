struct ControlResourceAllocation {
    let memoryMiB: Int
    let cpu: Int

    init?(memoryGiB: Double, cpu: Double) {
        guard memoryGiB.isFinite, cpu.isFinite,
              memoryGiB >= Double(VMResourceLimits.minimumMemoryMiB) / 1024,
              memoryGiB <= Double(VMResourceLimits.maximumMemoryMiB) / 1024,
              cpu.rounded() == cpu,
              cpu >= Double(VMResourceLimits.minimumCPU),
              cpu <= Double(VMResourceLimits.maximumCPU) else { return nil }
        memoryMiB = Int((memoryGiB * 1024).rounded())
        self.cpu = Int(cpu)
    }
}
