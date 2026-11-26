import Foundation

/// Recognized containers on both Windows import forms. The converter validates each supported format.
enum HvfImportedDiskFormat: String {
    case qcow2 = "QCOW2"
    case vhdx = "VHDX"
    case vhd = "VHD"
    case nonFixedVHD = "Hyper-V VHD"
    case vdi = "VDI"
    case vmdk = "VMDK"

    var importError: FirstRunImport.ValidationError? {
        switch self {
        case .vdi, .vmdk: return .unsupportedDiskFormat(rawValue)
        case .qcow2, .vhdx, .vhd, .nonFixedVHD: return nil
        }
    }

    func conversionLabel(percent: Int) -> String {
        "\(rawValue) 디스크를 변환하는 중…" + (percent > 0 ? " \(percent)%" : "")
    }

    static func detect(path: String) -> Self? {
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return nil }
        defer { try? handle.close() }
        let header = (try? handle.read(upToCount: 72)) ?? Data()
        if header.starts(with: Data([0x51, 0x46, 0x49, 0xfb])) { return .qcow2 }
        if header.starts(with: Data("vhdxfile".utf8)) { return .vhdx }
        // Sparse VMDK version 1. VDI 1.1 carries its signature and version after a 64-byte comment.
        if header.starts(with: [0x4b, 0x44, 0x4d, 0x56, 1, 0, 0, 0]) { return .vmdk }
        if header.dropFirst(64).starts(with: [0x7f, 0x10, 0xda, 0xbe, 1, 0, 1, 0]) { return .vdi }
        // Fixed VHD has no leading signature. The version-1 footer identifies it
        // independently of the filename; the native converter checks its checksum and layout.
        guard let end = try? handle.seekToEnd(), end >= 512,
              (try? handle.seek(toOffset: end - 512)) != nil,
              let footer = try? handle.read(upToCount: 64), footer.count == 64,
              footer.starts(with: "conectix".utf8), footer[12..<16].elementsEqual([0, 1, 0, 0]) else { return nil }
        return footer[60..<64].elementsEqual([0, 0, 0, 2]) ? .vhd : .nonFixedVHD
    }
}
