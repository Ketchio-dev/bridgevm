#if canImport(AppKit)
import Foundation
import IOSurface

struct HvfIOSurfaceDescriptor: Equatable {
    let id: IOSurfaceID
    let width: Int
    let height: Int

    static func parse(_ text: String) -> HvfIOSurfaceDescriptor? {
        let fields = text.split(whereSeparator: { $0.isWhitespace })
        guard fields.count == 3,
              let rawID = UInt32(fields[0]), rawID != 0,
              let width = Int(fields[1]), width > 0,
              let height = Int(fields[2]), height > 0
        else {
            return nil
        }
        return HvfIOSurfaceDescriptor(id: rawID, width: width, height: height)
    }

    static func load(from url: URL) -> HvfIOSurfaceDescriptor? {
        guard let data = try? Data(contentsOf: url),
              data.count <= 128,
              let text = String(data: data, encoding: .utf8)
        else {
            return nil
        }
        return parse(text)
    }
}

struct HvfIOSurfaceScanout {
    let descriptor: HvfIOSurfaceDescriptor
    let surface: IOSurfaceRef

    static func lookup(_ descriptor: HvfIOSurfaceDescriptor) -> HvfIOSurfaceScanout? {
        guard let surface = IOSurfaceLookup(descriptor.id) else {
            return nil
        }

        let actualWidth = IOSurfaceGetWidth(surface)
        let actualHeight = IOSurfaceGetHeight(surface)
        guard actualWidth == descriptor.width,
              actualHeight == descriptor.height else {
            return nil
        }

        return HvfIOSurfaceScanout(descriptor: descriptor, surface: surface)
    }
}
#endif
