import ImageIO
import SwiftUI
import UniformTypeIdentifiers

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

/// The Images of a Description (ADR-0029), as the editor needs them: it never knows where they are
/// kept. A picture goes in as the bytes of any file ImageIO reads, and its reference comes out.
struct ImageSource {
    var add: (Data) -> String?
    var read: (String) -> Data?
}

/// The bytes an Image is stored as.
enum StoredImage {
    /// The formats kept byte for byte: those every browser reads.
    private static let kept: Set<UTType> = [.png, .jpeg, .gif, .webP]

    /// `picture` as it is stored: as it is if it's a PNG, JPEG, GIF or WebP, else (TIFF, HEIC...)
    /// as PNG. Nil if it is no image ImageIO reads.
    static func bytes(of picture: Data) -> Data? {
        guard let source = CGImageSourceCreateWithData(picture as CFData, nil) else { return nil }
        if let type = CGImageSourceGetType(source).flatMap({ UTType($0 as String) }), kept.contains(type) {
            return picture
        }
        guard let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        let png = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(png, UTType.png.identifier as CFString, 1, nil)
        else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination) ? png as Data : nil
    }

    /// Puts the stored `bytes` of an Image on `pasteboard` as the type they are; on the Mac also as
    /// TIFF unless PNG, the picture types every app there reads.
    static func copy(_ bytes: Data, to pasteboard: Pasteboard) {
        let type =
            CGImageSourceCreateWithData(bytes as CFData, nil).flatMap(CGImageSourceGetType)
            .flatMap { UTType($0 as String) } ?? .png
        #if canImport(UIKit)
            pasteboard.setData(bytes, forPasteboardType: type.identifier)
        #else
            pasteboard.clearContents()
            pasteboard.setData(bytes, forType: NSPasteboard.PasteboardType(type.identifier))
            if type != .png, type != .tiff, let tiff = NSImage(data: bytes)?.tiffRepresentation {
                pasteboard.setData(tiff, forType: .tiff)
            }
        #endif
    }
}

/// An Image shown in the text, at most as wide as the line.
private final class FittedAttachment: NSTextAttachment {
    let reference: String

    init(reference: String) {
        self.reference = reference
        super.init(data: nil, ofType: nil)
    }

    required init?(coder: NSCoder) { nil }

    override func attachmentBounds(
        for attributes: [NSAttributedString.Key: Any], location: any NSTextLocation,
        textContainer: NSTextContainer?, proposedLineFragment: CGRect, position: CGPoint
    ) -> CGRect {
        guard let size = image?.size, size.width > 0 else { return .zero }
        let width = min(size.width, proposedLineFragment.width)
        return CGRect(x: 0, y: 0, width: width, height: size.height * width / size.width)
    }
}

/// The attachments that draw a Description's Images, each decoded once.
final class ImageAttachments {
    /// Longest side decoded, so that a huge picture doesn't fill the memory.
    private static let maxPixelSize = 2048
    private let source: ImageSource
    private var attachments: [String: NSTextAttachment] = [:]

    init(_ source: ImageSource) { self.source = source }

    /// What draws the Image `reference` names; nil if it can't be read, as before it has Synced.
    func attachment(for reference: String) -> NSTextAttachment? {
        if let known = attachments[reference] { return known }
        guard let data = source.read(reference), let image = Self.decode(data) else { return nil }
        let attachment = FittedAttachment(reference: reference)
        attachment.image = image
        attachments[reference] = attachment
        return attachment
    }

    /// Puts the Image `attachment` draws on the general pasteboard, as it is stored.
    func copy(_ attachment: NSTextAttachment) {
        guard let reference = (attachment as? FittedAttachment)?.reference, let bytes = source.read(reference)
        else { return }
        StoredImage.copy(bytes, to: .general)
    }

    private static func decode(_ data: Data) -> PlatformImage? {
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
        ]
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
            let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
        else { return nil }
        #if canImport(UIKit)
            return UIImage(cgImage: image)
        #else
            return NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
        #endif
    }
}

#if canImport(UIKit)
    typealias PlatformImage = UIImage
    typealias Pasteboard = UIPasteboard
#else
    typealias PlatformImage = NSImage
    typealias Pasteboard = NSPasteboard
#endif
