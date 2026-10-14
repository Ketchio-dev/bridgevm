import Foundation

/// Known container formats that cannot be imported as an installed RAW disk.
/// This signature check is shared by both Windows HVF import entry points.
enum HvfImportedDiskFormat: String {
    case qcow2 = "QCOW2"
    case vhdx = "VHDX"

    var importError: String {
        "\(rawValue) 이미지는 가져올 수 없습니다. 설치된 RAW 디스크를 선택하세요."
    }

    static func detect(path: String) -> Self? {
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return nil }
        defer { try? handle.close() }
        let header = (try? handle.read(upToCount: 8)) ?? Data()
        if header.starts(with: Data([0x51, 0x46, 0x49, 0xfb])) { return .qcow2 }
        if header.starts(with: Data("vhdxfile".utf8)) { return .vhdx }
        return nil
    }
}
