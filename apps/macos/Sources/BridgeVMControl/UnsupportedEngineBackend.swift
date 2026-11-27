import Foundation

/// A VM saved with a removed engine (Fast VZ or QEMU). It stays in the library so
/// the user can see it and delete it, but nothing here starts, stops or writes
/// anything: its files are never touched automatically.
final class UnsupportedEngineBackend: VMBackend {
    let config: VMConfig
    init(_ config: VMConfig) { self.config = config }

    func isRunning() -> Bool { false }
    func resources() -> (memMiB: Int, cpu: Int) { (config.memMiB ?? 0, config.cpuCount ?? 0) }
}
