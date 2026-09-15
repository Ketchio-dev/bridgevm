import Foundation
import Darwin

enum NativeLibraryConfigLoader {
    static func readConfig(rootDescriptor: Int32, id: String) throws -> VMConfig {
        guard NativeLibraryReader.isCanonicalID(id) else { throw NativeCLIError.invalid("Invalid VM ID.") }
        let directory = openat(rootDescriptor, id, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { throw NativeCLIError.unavailable("Cannot open the VM directory safely.") }
        defer { close(directory) }
        let descriptor = openat(directory, "vm.json", O_RDONLY | O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC)
        guard descriptor >= 0 else { throw NativeCLIError.unavailable("Cannot open vm.json as a regular file.") }
        let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
        defer { try? handle.close() }
        var metadata = stat()
        let limit = VMLibrary.maximumConfigBytes
        guard fstat(descriptor, &metadata) == 0, metadata.st_mode & S_IFMT == S_IFREG,
              metadata.st_size >= 0, metadata.st_size <= limit else {
            throw NativeCLIError.unavailable("vm.json must be a regular file of at most \(limit) bytes.")
        }
        var data = Data()
        while data.count <= limit {
            guard let chunk = try handle.read(upToCount: min(65_536, limit + 1 - data.count)),
                  !chunk.isEmpty else { break }
            data.append(chunk)
        }
        guard data.count <= limit else { throw NativeCLIError.unavailable("vm.json exceeded the size limit while reading.") }
        var config = try JSONDecoder().decode(VMConfig.self, from: data)
        config.id = id
        return config
    }

}
