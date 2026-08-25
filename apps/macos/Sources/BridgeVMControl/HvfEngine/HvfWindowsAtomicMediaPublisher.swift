import Darwin
import Foundation
enum HvfWindowsAtomicMediaPublisher {
    enum PublicationError: Error { case stagingFailed, swapFailed(Int32), rollbackFailed }
    struct Staged {
        let destination: String, source: String, temporary: String
        let replacesExisting: Bool
    }
    static func stage(destination: String, source: String) throws -> Staged {
        let fm = FileManager.default
        let temporary = destination + ".staging-\(UUID().uuidString)"
        if Shell.run("/bin/cp", ["-c", source, temporary]).code != 0 {
            do { try fm.copyItem(atPath: source, toPath: temporary) }
            catch { try? fm.removeItem(atPath: temporary); throw PublicationError.stagingFailed }
        }
        return .init(destination: destination, source: source, temporary: temporary,
                     replacesExisting: fm.fileExists(atPath: destination))
    }
    static func swap(_ item: Staged) throws {
        if item.replacesExisting {
            guard renameatx_np(AT_FDCWD, item.temporary, AT_FDCWD, item.destination,
                UInt32(RENAME_SWAP)) == 0 else { throw PublicationError.swapFailed(errno) }
        } else { try FileManager.default.moveItem(
            atPath: item.temporary, toPath: item.destination) }
    }
    static func rollback(_ item: Staged) throws {
        if item.replacesExisting {
            guard renameatx_np(AT_FDCWD, item.temporary, AT_FDCWD,
                item.destination, UInt32(RENAME_SWAP)) == 0 else {
                throw PublicationError.rollbackFailed
            }
        } else { try FileManager.default.moveItem(
            atPath: item.destination, toPath: item.temporary) }
    }
}
