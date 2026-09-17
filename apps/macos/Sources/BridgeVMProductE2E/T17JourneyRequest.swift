import Foundation

protocol T17JourneyRequest {
    var jobID: String { get }
    var commit: String { get }
    var lane: Int { get }
    var nonce: String { get }
    var vmSlug: String { get }
    var libraryRootPath: String { get }
    var sharePath: String { get }
    var diskPath: String { get }
    var varsPath: String { get }
    var snapshotPath: String { get }
    var guestEvidencePath: String { get }
    var bundlePath: String { get }
}

extension T17Request: T17JourneyRequest {
    var bundlePath: String { URL(fileURLWithPath: diskPath).deletingLastPathComponent().deletingLastPathComponent().path }
}

extension A9ImportRequest {
    var bundlePath: String { URL(fileURLWithPath: diskPath).deletingLastPathComponent().deletingLastPathComponent().path }
}
