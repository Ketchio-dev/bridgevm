import Foundation

/// The drive a notice names for the VM's disk, by the last part of its mount point (`/Volumes/<name>`).
/// Nil for the Mac's startup disk, which notices call "your Mac": the probe's statfs reports it as its data
/// volume, `/System/Volumes/Data`, and Foundation as `/`. An empty mount point is one the host could not say.
enum HvfVMDrive {
    static func name(mountPoint: String) -> String? {
        guard mountPoint.hasPrefix("/"), mountPoint != "/", !mountPoint.hasPrefix("/System/Volumes/") else { return nil }
        let name = (mountPoint as NSString).lastPathComponent
        return name.isEmpty || name == "/" ? nil : name
    }

    /// The same for the drive holding `file`.
    static func name(of file: URL) -> String? {
        (try? file.resourceValues(forKeys: [.volumeURLKey]).volume).flatMap { name(mountPoint: $0.path) }
    }
}
