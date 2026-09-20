import Foundation

enum NativeTestPython {
    static func executable(
        environment: [String: String] = ProcessInfo.processInfo.environment,
        isExecutable: (String) -> Bool = FileManager.default.isExecutableFile(atPath:)
    ) throws -> URL {
        var candidates: [URL] = []
        if let developer = environment["DEVELOPER_DIR"], developer.hasPrefix("/") {
            candidates.append(URL(fileURLWithPath: developer).appendingPathComponent("usr/bin/python3"))
        }
        candidates += (environment["PATH"] ?? "").split(separator: ":").map {
            URL(fileURLWithPath: String($0)).appendingPathComponent("python3")
        }
        guard let python = candidates.first(where: { isExecutable($0.path) }) else {
            throw CocoaError(.executableNotLoadable)
        }
        return python
    }
}
