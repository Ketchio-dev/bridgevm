import Darwin
import Foundation

enum NativeCLIStatusConfiguration {
    static func read(library: NativeRuntimeLibraryHandle, id: String) -> NativeRuntimeSavedConfiguration {
        var entry = stat()
        if fstatat(library.descriptor, id, &entry, AT_SYMLINK_NOFOLLOW) != 0 {
            return .init(state: errno == ENOENT ? .missing : .unreadable, digest: nil)
        }
        guard entry.st_mode & S_IFMT == S_IFDIR else { return .init(state: .unreadable, digest: nil) }
        let directory = openat(library.descriptor, id, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { return .init(state: .unreadable, digest: nil) }
        defer { close(directory) }
        if fstatat(directory, "vm.json", &entry, AT_SYMLINK_NOFOLLOW) != 0 {
            return .init(state: errno == ENOENT ? .missing : .unreadable, digest: nil)
        }
        do {
            let config = try NativeCLIStatusConfigReader.read(library: library, id: id)
            return .init(state: .present, digest: try NativeRuntimeConfigurationIdentity.digest(config: config))
        } catch { return .init(state: .unreadable, digest: nil) }
    }
}
