import Foundation

/// Container disks both Windows HVF import entry points convert to the bundle's sparse RAW disk
/// with the helper's `convert-to-raw` (QEMU-based apps store QCOW2, Hyper-V VHDX). Any other file is a RAW
/// disk and keeps the clone-or-copy path. The helper reads and checks the whole image itself.
enum HvfImportedDiskFormat: String {
    case qcow2 = "QCOW2"
    case vhdx = "VHDX"

    func conversionLabel(percent: Int) -> String {
        "\(rawValue) 디스크를 변환하는 중…" + (percent > 0 ? " \(percent)%" : "")
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
