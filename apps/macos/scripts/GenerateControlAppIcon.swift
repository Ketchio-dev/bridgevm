import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

/// Pure bitmap rendering; shares the product mark and never starts AppKit.
@main
enum GenerateControlAppIcon {
    static func main() throws {
        guard CommandLine.arguments.count == 2 else { throw Failure.invalidArguments }
        let directory = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        for points in [16, 32, 128, 256, 512] {
            for scale in [1, 2] {
                let name = "icon_\(points)x\(points)\(scale == 2 ? "@2x" : "").png"
                try render(pixels: points * scale, to: directory.appendingPathComponent(name))
            }
        }
    }

    private static func render(pixels: Int, to destination: URL) throws {
        guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(data: nil, width: pixels, height: pixels,
                                      bitsPerComponent: 8, bytesPerRow: pixels * 4,
                                      space: colorSpace,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
            throw Failure.bitmapCreation
        }
        let size = CGFloat(pixels)
        context.scaleBy(x: size / 1024, y: size / 1024)
        // The same top-left coordinate system as SwiftUI's Shape.
        context.translateBy(x: 0, y: 1024)
        context.scaleBy(x: 1, y: -1)
        let tile = CGPath(roundedRect: CGRect(x: 64, y: 64, width: 896, height: 896),
                          cornerWidth: 200, cornerHeight: 200, transform: nil)
        context.addPath(tile)
        context.clip()
        guard let gradient = CGGradient(colorsSpace: colorSpace,
                                        colors: [CGColor(red: 0.12, green: 0.46, blue: 1, alpha: 1),
                                                 CGColor(red: 0.04, green: 0.25, blue: 0.84, alpha: 1)] as CFArray,
                                        locations: [0, 1]) else { throw Failure.bitmapCreation }
        context.drawLinearGradient(gradient, start: CGPoint(x: 512, y: 64),
                                   end: CGPoint(x: 512, y: 960), options: [])
        context.setFillColor(CGColor(gray: 1, alpha: 1))
        context.addPath(BridgeVMMark.cgPath(in: CGRect(x: 144, y: 132, width: 736, height: 736)))
        context.fillPath()
        guard let image = context.makeImage(),
              let output = CGImageDestinationCreateWithURL(destination as CFURL,
                                                          UTType.png.identifier as CFString, 1, nil) else {
            throw Failure.bitmapCreation
        }
        CGImageDestinationAddImage(output, image, nil)
        guard CGImageDestinationFinalize(output) else { throw Failure.writeFailed }
    }

    private enum Failure: Error { case invalidArguments, bitmapCreation, writeFailed }
}
