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

    /// Decode P6 without copying the file into an array first and without
    /// growing the output a byte at a time.
    ///
    /// A full-screen capture is millions of pixels, so `[UInt8](data)` plus four
    /// `Data.append` calls per pixel dominated everything else: a real 1920x1080
    /// capture took 162 ms per frame that way against 1.4 ms here. This runs on
    /// the main actor from the session poll, so that cost was UI stall.
    static func decode(data: Data) throws -> (width: Int, height: Int, rgba: Data) {
        try data.withUnsafeBytes { raw -> (width: Int, height: Int, rgba: Data) in
            let bytes = raw.bindMemory(to: UInt8.self)
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
                return String(decoding: bytes[start..<index], as: UTF8.self)
            }

            guard nextToken() == "P6",
                  let widthText = nextToken(), let width = Int(widthText),
                  let heightText = nextToken(), let height = Int(heightText),
                  let maxText = nextToken(), Int(maxText) == 255 else {
                throw PpmDecoderError.invalidHeader
            }
            // P6 ends the header with exactly one whitespace byte after the
            // maxval; everything after it is pixel data. Skipping a run of
            // whitespace here would eat pixel bytes that happen to be 9, 10,
            // 13 or 32, shifting the whole image by a channel.
            guard index < bytes.count,
                  bytes[index] == 9 || bytes[index] == 10
                    || bytes[index] == 13 || bytes[index] == 32 else {
                throw PpmDecoderError.invalidHeader
            }
            index += 1
            guard width > 0, height > 0 else { throw PpmDecoderError.invalidHeader }
            let pixelCount = width * height
            guard bytes.count - index >= pixelCount * 3 else { throw PpmDecoderError.truncatedPixels }
            var rgba = Data(count: pixelCount * 4)
            rgba.withUnsafeMutableBytes { destination in
                let out = destination.bindMemory(to: UInt8.self)
                var source = index
                var target = 0
                for _ in 0..<pixelCount {
                    out[target] = bytes[source]
                    out[target + 1] = bytes[source + 1]
                    out[target + 2] = bytes[source + 2]
                    out[target + 3] = 255
                    source += 3
                    target += 4
                }
            }
            return (width, height, rgba)
        }
    }
}

enum PpmDecoderError: Error {
    case invalidHeader
    case truncatedPixels
}
