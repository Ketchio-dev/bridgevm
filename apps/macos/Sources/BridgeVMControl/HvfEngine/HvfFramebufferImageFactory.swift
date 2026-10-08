#if canImport(AppKit)
import CoreGraphics
import Darwin

struct HvfFramebufferImageFactory {
    var provider: (UnsafeMutableRawPointer, Int) -> CGDataProvider? = { buffer, byteCount in
        CGDataProvider(dataInfo: buffer, data: buffer, size: byteCount,
                       releaseData: { info, _, _ in if let info { free(info) } })
    }
    var image: (CGDataProvider, Int, Int, Int) -> CGImage? = { provider, width, height, stride in
        CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                bytesPerRow: stride, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGBitmapInfo.byteOrder32Little.rawValue
                    | CGImageAlphaInfo.noneSkipFirst.rawValue),
                provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
    }

    /// Takes ownership of the copied pixels, including either construction failure.
    func make(buffer: UnsafeMutableRawPointer, byteCount: Int,
              width: Int, height: Int, stride: Int) -> CGImage? {
        guard let provider = provider(buffer, byteCount) else {
            free(buffer)
            return nil
        }
        return image(provider, width, height, stride)
    }
}
#endif
