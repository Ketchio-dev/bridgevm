import Foundation

enum LibraryDeletionTestDirectory {
    static var temporary: URL {
        URL(fileURLWithPath: ProcessInfo.processInfo.environment["TMPDIR"]
            ?? FileManager.default.temporaryDirectory.path)
    }
}
