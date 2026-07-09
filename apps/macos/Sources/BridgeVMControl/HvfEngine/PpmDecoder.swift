import Foundation
#if canImport(AppKit)
import AppKit
#endif

enum PpmDecoder {
    #if canImport(AppKit)
    static func decodeImage(at url: URL) -> NSImage? {
        guard let decoded = try? decode(data: Data(contentsOf: url)) else { return nil }
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        guard let provider = CGDataProvider(data: decoded.rgba as CFData),
              let cg = CGImage(width: decoded.width,
                               height: decoded.height,
                               bitsPerComponent: 8,
                               bitsPerPixel: 32,
                               bytesPerRow: decoded.width * 4,
                               space: colorSpace,
                               bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                               provider: provider,
                               decode: nil,
                               shouldInterpolate: false,
                               intent: .defaultIntent) else { return nil }
        return NSImage(cgImage: cg, size: NSSize(width: decoded.width, height: decoded.height))
    }
    #endif

    static func decode(data: Data) throws -> (width: Int, height: Int, rgba: Data) {
        let bytes = [UInt8](data)
        var index = 0

        func nextToken() -> String? {
            while index < bytes.count {
                let b = bytes[index]
                if b == 35 {
                    while index < bytes.count, bytes[index] != 10 { index += 1 }
                } else if b == 9 || b == 10 || b == 13 || b == 32 {
                    index += 1
                } else {
                    break
                }
            }
            guard index < bytes.count else { return nil }
            let start = index
            while index < bytes.count {
                let b = bytes[index]
                if b == 9 || b == 10 || b == 13 || b == 32 || b == 35 { break }
                index += 1
            }
            return String(bytes: bytes[start..<index], encoding: .ascii)
        }

        guard nextToken() == "P6",
              let widthText = nextToken(), let width = Int(widthText),
              let heightText = nextToken(), let height = Int(heightText),
              let maxText = nextToken(), Int(maxText) == 255 else {
            throw PpmDecoderError.invalidHeader
        }
        while index < bytes.count, [UInt8(9), 10, 13, 32].contains(bytes[index]) { index += 1 }
        let pixelCount = width * height
        guard bytes.count - index >= pixelCount * 3 else { throw PpmDecoderError.truncatedPixels }
        var rgba = Data(capacity: pixelCount * 4)
        for offset in stride(from: index, to: index + pixelCount * 3, by: 3) {
            rgba.append(bytes[offset])
            rgba.append(bytes[offset + 1])
            rgba.append(bytes[offset + 2])
            rgba.append(255)
        }
        return (width, height, rgba)
    }
}

enum PpmDecoderError: Error {
    case invalidHeader
    case truncatedPixels
}
