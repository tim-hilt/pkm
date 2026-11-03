import CryptoKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// Prepares a pasted or dropped image for the core (ADR-0003): the core only
/// stores bytes, so the client makes sure they are within the limits Excalidraw
/// has for imports (1440 px on the long side, 4 MiB) and in a format
/// excalidraw.com can read.
enum ImageImporter {
    /// Longest side of an imported image in pixels; larger pictures are
    /// downscaled.
    static let maxPixelSize = 1440
    /// Largest file in bytes; larger ones are recompressed, and shrunk if that
    /// is not enough.
    static let maxFileSize = 4 * 1024 * 1024
    /// Shrinking stops here: a picture this small has been lost anyway.
    private static let minPixelSize = 32
    /// JPEG qualities to try, best first, at each size.
    private static let jpegQualities = [0.85, 0.6, 0.4]
    /// The formats that stay as they are when within the limits: those every
    /// browser reads, so that the Drawing opens on excalidraw.com.
    private static let webFormats: [UTType: String] = [
        .png: "image/png", .jpeg: "image/jpeg", .gif: "image/gif", .webP: "image/webp",
    ]

    /// The image in `data` ready to be inserted into a Drawing, or nil if the
    /// bytes are not an image ImageIO understands.
    ///
    /// An image within the limits in a format browsers read is kept exactly as
    /// it is. Any other is decoded, turned upright by its EXIF orientation,
    /// downscaled and encoded again: as PNG if it is a PNG or has transparency,
    /// else as JPEG. If that is still too large it is shrunk by a quarter at a
    /// time (and JPEGs are compressed harder) until it fits.
    static func prepare(_ data: Data) -> ImageFile? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
            let width = properties[kCGImagePropertyPixelWidth] as? Int,
            let height = properties[kCGImagePropertyPixelHeight] as? Int,
            width > 0, height > 0
        else { return nil }
        let id = fileID(for: data)
        let type = CGImageSourceGetType(source).flatMap { UTType($0 as String) }
        // Sideways pictures are shown upright by browsers: their natural size
        // is the turned one.
        let sideways = (properties[kCGImagePropertyOrientation] as? Int ?? 1) >= 5
        let natural = sideways ? (width: height, height: width) : (width: width, height: height)
        let longSide = max(natural.width, natural.height)

        if let mimeType = type.flatMap({ webFormats[$0] }), longSide <= maxPixelSize, data.count <= maxFileSize {
            return ImageFile(
                fileId: id, mimeType: mimeType, bytes: data, width: UInt32(natural.width),
                height: UInt32(natural.height))
        }

        let hasAlpha = properties[kCGImagePropertyHasAlpha] as? Bool ?? false
        let output: UTType = hasAlpha || type?.conforms(to: .png) == true ? .png : .jpeg
        let qualities = output == .png ? [1.0] : jpegQualities
        var side = min(longSide, maxPixelSize)
        while side >= minPixelSize {
            guard let image = thumbnail(of: source, maxPixelSize: side) else { return nil }
            for quality in qualities {
                if let encoded = encode(image, as: output, quality: quality), encoded.count <= maxFileSize {
                    return ImageFile(
                        fileId: id, mimeType: output == .png ? "image/png" : "image/jpeg", bytes: encoded,
                        width: UInt32(image.width), height: UInt32(image.height))
                }
            }
            side = side * 3 / 4
        }
        return nil
    }

    /// What excalidraw.com calls the file: the SHA-1 of its bytes, as lower-case
    /// hex. Taken of the original, before any downscaling, so that the same
    /// picture is stored once however it was shrunk.
    static func fileID(for data: Data) -> String {
        Insecure.SHA1.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    /// The picture at most `maxPixelSize` px on its long side, upright. ImageIO
    /// downsamples while decoding, so the full-size bitmap is never held.
    private static func thumbnail(of source: CGImageSource, maxPixelSize: Int) -> CGImage? {
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    private static func encode(_ image: CGImage, as type: UTType, quality: Double) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(
            destination, image, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        return CGImageDestinationFinalize(destination) ? data as Data : nil
    }
}
