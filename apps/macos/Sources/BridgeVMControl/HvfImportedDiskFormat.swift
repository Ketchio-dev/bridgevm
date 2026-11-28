import Foundation

/// Recognized containers on both Windows import forms. The converter validates each supported format.
enum HvfImportedDiskFormat: String {
    case qcow2 = "QCOW2"
    case vhdx = "VHDX"
    case vhd = "VHD"
    case nonFixedVHD = "Hyper-V VHD"
    case vdi = "VDI"
    case vmdk = "VMDK"
    case parallels = "Parallels"

    var importError: FirstRunImport.ValidationError? {
        switch self {
        case .vdi, .vmdk, .parallels: return .unsupportedDiskFormat(rawValue)
        case .qcow2, .vhdx, .vhd, .nonFixedVHD: return nil
        }
    }

    func conversionLabel(percent: Int) -> String {
        "Converting \(rawValue) disk…" + (percent > 0 ? " \(percent)%" : "")
    }

    static func detect(path: String) -> Self? {
        guard let handle = try? FileHandle(forReadingFrom: URL(fileURLWithPath: path)) else { return nil }
        defer { try? handle.close() }
        let header = (try? handle.read(upToCount: 72)) ?? Data()
        if header.starts(with: Data([0x51, 0x46, 0x49, 0xfb])) { return .qcow2 }
        if header.starts(with: Data("vhdxfile".utf8)) { return .vhdx }
        // Any sparse VMDK (version 1, 2, or 3: streamOptimized, as in an OVA), and a VMDK descriptor,
        // the small text file beside its data files. Only qemu-img's default (version 1) was known:
        // the others were taken for RAW disks, giving a tiny VM and "no boot partition".
        // VDI 1.1 carries its signature and version after a 64-byte comment.
        if header.starts(with: "KDMV".utf8) || header.starts(with: "# Disk DescriptorFile".utf8) { return .vmdk }
        // A Parallels expanding disk (the .hds file inside a .hdd folder).
        if header.starts(with: "WithoutFreeSpace".utf8) || header.starts(with: "WithouFreSpacExt".utf8) { return .parallels }
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
