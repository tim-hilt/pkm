import CoreGraphics
import Foundation
import ImageIO

/// Pixels decoded from an image file (ADR-0003: the core only stores the
/// file's bytes; decoding is the client's job).
struct DecodedImage {
    let width: Int
    let height: Int
    /// RGBA, 8 bits per channel, sRGB, alpha premultiplied; rows are tightly
    /// packed, the first row is the top of the picture.
    let pixels: [UInt8]
}

enum ImageDecoder {
    /// Longest side of a decoded image in pixels; larger pictures are
    /// downscaled (Excalidraw itself limits imports to 1440). Bounds memory
    /// and keeps every image within what any GPU takes as a texture.
    static let maxPixelSize = 4096

    /// Decodes the PNG, JPEG, GIF, WebP, HEIC... in `data` with ImageIO,
    /// turned upright by its EXIF orientation like browsers show it. Nil if
    /// the bytes are not an image ImageIO understands (SVG, for one).
    static func decode(_ data: Data, maxPixelSize: Int = maxPixelSize) -> DecodedImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
            let width = properties[kCGImagePropertyPixelWidth] as? Int,
            let height = properties[kCGImagePropertyPixelHeight] as? Int,
            width > 0, height > 0
        else { return nil }
        // The thumbnail API is how ImageIO downsamples while decoding, without
        // first holding the full-size bitmap; at full size it only applies the
        // orientation.
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: min(max(width, height), maxPixelSize),
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        else { return nil }
        return premultipliedRGBA(of: image)
    }

    private static func premultipliedRGBA(of image: CGImage) -> DecodedImage? {
        let (width, height) = (image.width, image.height)
        guard width > 0, height > 0, let colorSpace = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard
                let context = CGContext(
                    data: buffer.baseAddress, width: width, height: height, bitsPerComponent: 8,
                    bytesPerRow: width * 4, space: colorSpace,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
            else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        return drawn ? DecodedImage(width: width, height: height, pixels: pixels) : nil
    }
}
