import Foundation

struct BridgeVMControlLaunchOptions: Equatable {
    let e2eLibraryRoot: URL?
    let e2eUnattendedPath: URL?

    enum ParseError: LocalizedError, Equatable {
        case missingLibraryRoot
        case duplicateLibraryRoot
        case relativeLibraryRoot(String)
        case unsafeLibraryRoot(String)
        case unavailableLibraryRoot(String)
        case missingUnattendedPath
        case duplicateUnattendedPath
        case unattendedWithoutLibraryRoot
        case unsafeUnattendedPath(String)

        var errorDescription: String? {
            switch self {
            case .missingLibraryRoot:
                return "--e2e-library-root requires an absolute path"
            case .duplicateLibraryRoot:
                return "--e2e-library-root may be specified only once"
            case let .relativeLibraryRoot(path):
                return "--e2e-library-root must be absolute: \(path)"
            case let .unsafeLibraryRoot(path):
                return "--e2e-library-root must be an existing, empty, non-symlink /tmp/bridgevm-e2e-* directory: \(path)"
            case let .unavailableLibraryRoot(path):
                return "--e2e-library-root cannot be inspected: \(path)"
            case .missingUnattendedPath:
                return "--e2e-unattend-path requires an absolute path"
            case .duplicateUnattendedPath:
                return "--e2e-unattend-path may be specified only once"
            case .unattendedWithoutLibraryRoot:
                return "--e2e-unattend-path requires --e2e-library-root"
            case let .unsafeUnattendedPath(path):
                return "--e2e-unattend-path must be a regular non-symlink file beside the isolated E2E library: \(path)"
            }
        }
    }

    static func parse(
        arguments: [String],
        fileManager: FileManager = .default
    ) throws -> BridgeVMControlLaunchOptions {
        var root: URL?
        var unattendedRaw: String?
        var index = 0
        while index < arguments.count {
            if arguments[index] == "--e2e-unattend-path" {
                guard unattendedRaw == nil else { throw ParseError.duplicateUnattendedPath }
                guard index + 1 < arguments.count else { throw ParseError.missingUnattendedPath }
                unattendedRaw = arguments[index + 1]
                index += 2
                continue
            }
            guard arguments[index] == "--e2e-library-root" else {
                index += 1
                continue
            }
            guard root == nil else { throw ParseError.duplicateLibraryRoot }
            guard index + 1 < arguments.count else { throw ParseError.missingLibraryRoot }
            let raw = arguments[index + 1]
            guard raw.hasPrefix("/") else { throw ParseError.relativeLibraryRoot(raw) }
            root = try validateE2ERoot(raw, fileManager: fileManager)
            index += 2
        }
        let unattended: URL?
        if let raw = unattendedRaw {
            guard let root else { throw ParseError.unattendedWithoutLibraryRoot }
            unattended = try validateUnattended(raw, libraryRoot: root)
        } else {
            unattended = nil
        }
        return BridgeVMControlLaunchOptions(
            e2eLibraryRoot: root, e2eUnattendedPath: unattended)
    }

    private static func validateUnattended(_ raw: String, libraryRoot: URL) throws -> URL {
        guard raw.hasPrefix("/"), !(raw as NSString).pathComponents.contains("..") else {
            throw ParseError.unsafeUnattendedPath(raw)
        }
        let lexical = URL(fileURLWithPath: raw).standardizedFileURL
        let canonical = lexical.resolvingSymlinksInPath()
        let expectedParent = libraryRoot.deletingLastPathComponent()
        guard lexical.path == canonical.path,
              canonical.deletingLastPathComponent() == expectedParent,
              let values = try? lexical.resourceValues(
                forKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey]),
              values.isRegularFile == true, values.isSymbolicLink != true,
              let size = values.fileSize, size > 0, size <= 1_048_576 else {
            throw ParseError.unsafeUnattendedPath(raw)
        }
        return canonical
    }

    static func validateOrExit(arguments: [String]) {
        do {
            _ = try parse(arguments: arguments)
        } catch {
            FileHandle.standardError.write(
                Data("BridgeVMControl: \(error.localizedDescription)\n".utf8))
            exit(2)
        }
    }

    private static func validateE2ERoot(
        _ raw: String,
        fileManager: FileManager
    ) throws -> URL {
        guard !(raw as NSString).pathComponents.contains("..") else {
            throw ParseError.unsafeLibraryRoot(raw)
        }
        let lexical = URL(fileURLWithPath: raw, isDirectory: true).standardizedFileURL
        let canonical = lexical.resolvingSymlinksInPath()
        guard canonical.path.hasPrefix("/tmp/bridgevm-e2e-") else {
            throw ParseError.unsafeLibraryRoot(raw)
        }
        do {
            let values = try lexical.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            guard values.isDirectory == true, values.isSymbolicLink != true,
                  lexical.path == canonical.path,
                  try fileManager.contentsOfDirectory(atPath: lexical.path).isEmpty else {
                throw ParseError.unsafeLibraryRoot(raw)
            }
        } catch let error as ParseError {
            throw error
        } catch {
            throw ParseError.unavailableLibraryRoot(raw)
        }
        return canonical
    }
}
