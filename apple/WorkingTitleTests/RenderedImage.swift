import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// A rendered frame in memory: 8-bit sRGB pixels, which is what the renderer
/// writes and what reference PNGs store. Knows how to become a PNG and back,
/// so no other test code touches ImageIO.
struct RenderedImage: Equatable {
    static let bytesPerPixel = 4

    let width: Int
    let height: Int
    /// Premultiplied BGRA, rows top to bottom, tightly packed.
    let bgra: [UInt8]

    init(width: Int, height: Int, bgra: [UInt8]) {
        precondition(bgra.count == width * height * Self.bytesPerPixel, "pixel count does not match the size")
        self.width = width
        self.height = height
        self.bgra = bgra
    }

    /// Brightness of the red channel at device pixel `(x, y)`: enough to tell
    /// ink from paper in grey drawings.
    func red(_ x: Int, _ y: Int) -> UInt8 { bgra[(y * width + x) * Self.bytesPerPixel + 2] }

    /// Nil if `data` is not a decodable image.
    init?(pngData data: Data) {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        var bgra = [UInt8](repeating: 0, count: image.width * image.height * Self.bytesPerPixel)
        let drawn = bgra.withUnsafeMutableBytes { buffer in
            Self.context(over: buffer.baseAddress!, width: image.width, height: image.height)?
                .draw(image, in: CGRect(x: 0, y: 0, width: image.width, height: image.height))
        }
        guard drawn != nil else { return nil }
        self.init(width: image.width, height: image.height, bgra: bgra)
    }

    func pngData() -> Data {
        var pixels = bgra
        let image = pixels.withUnsafeMutableBytes { buffer in
            Self.context(over: buffer.baseAddress!, width: width, height: height)!.makeImage()!
        }
        let data = NSMutableData()
        let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, image, nil)
        precondition(CGImageDestinationFinalize(destination), "PNG encoding failed")
        return data as Data
    }

    /// A bitmap context drawing straight into `pixels` in the layout above.
    private static func context(over pixels: UnsafeMutableRawPointer, width: Int, height: Int) -> CGContext? {
        CGContext(
            data: pixels, width: width, height: height, bitsPerComponent: 8,
            bytesPerRow: width * bytesPerPixel, space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
    }
}
