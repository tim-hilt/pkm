import CoreGraphics
import Foundation
import ImageIO
import PDFKit
import UniformTypeIdentifiers

#if canImport(UIKit)
    import UIKit
#else
    import AppKit
#endif

/// Where a Book's Cover comes from.
enum CoverSource: Sendable {
    /// The first page of the PDF at this URL.
    case pdf(URL)
    /// The EPUB's cover `image`, if it has one; else a plain Cover with its Title and Author.
    case epub(title: String, author: String?, image: @Sendable () -> Data?)
}

/// The saved Cover of every Book: an image, one file per File content (the
/// Book's SHA-256) in `folder`, so a File edited outside the app gets a new
/// Cover. Meant for the system Caches folder, so Covers are never synced and
/// the system may clear them; `cover(key:source:)` then makes them again.
///
/// All methods read or write files and may render, so call them off the main
/// thread.
struct CoverStore: Sendable {
    let folder: URL
    /// A Cover is this wide: 300 pt at the scale of the densest display the app
    /// is on when the store is made. Smaller cells scale it down. (An EPUB's image is no wider.)
    private let pixelWidth: CGFloat

    /// Make one per use rather than keeping it: Covers made later then suit the
    /// displays in use by then.
    @MainActor init(folder: URL) {
        self.folder = folder
        pixelWidth = 300 * Self.densestDisplayScale
    }

    /// The highest scale among the app's windows (iOS) or the attached screens
    /// (macOS). The saved Covers serve all of them. Before any window exists,
    /// that of the densest display of the platform.
    @MainActor private static var densestDisplayScale: CGFloat {
        #if canImport(UIKit)
            UIApplication.shared.connectedScenes
                .compactMap { ($0 as? UIWindowScene)?.traitCollection.displayScale }
                .max() ?? 3
        #else
            NSScreen.screens.map(\.backingScaleFactor).max() ?? 2
        #endif
    }

    /// The Cover saved under `key`, a Book's SHA-256, or else one made from
    /// `source` and saved. Nil if it can't be made.
    func cover(key: String, source: CoverSource) -> CGImage? {
        if let saved = saved(key) { return saved }
        guard let cover = render(source) else { return nil }
        save(cover, key: key)
        return cover
    }

    private func render(_ source: CoverSource) -> CGImage? {
        switch source {
        case .pdf(let file):
            return PDFDocument(url: file).flatMap(render)
        case .epub(let title, let author, let image):
            return image().flatMap { EPUBCover.image(from: $0, pixelWidth: pixelWidth) }
                ?? EPUBCover.plain(title: title, author: author, pixelWidth: pixelWidth)
        }
    }

    /// Page 1 of `pdf` as a Cover, not saved. Nil if the PDF has no pages.
    func render(_ pdf: PDFDocument) -> CGImage? { pdf.cover(pixelWidth: pixelWidth) }

    /// Saves `cover` under `key`. A failed write is ignored: the Cover is rendered again when needed.
    func save(_ cover: CGImage, key: String) {
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        guard
            let destination = CGImageDestinationCreateWithURL(
                url(key) as CFURL, UTType.jpeg.identifier as CFString, 1, nil)
        else { return }
        CGImageDestinationAddImage(
            destination, cover, [kCGImageDestinationLossyCompressionQuality: 0.85] as CFDictionary)
        CGImageDestinationFinalize(destination)
    }

    /// Deletes the Cover saved under `key`, if any.
    func remove(_ key: String) {
        try? FileManager.default.removeItem(at: url(key))
    }

    /// The Cover saved under `key`, or nil if there is none. Never renders.
    /// Decoded here, so drawing it on the main thread doesn't stall scrolling.
    func saved(_ key: String) -> CGImage? {
        CGImageSourceCreateWithURL(url(key) as CFURL, nil).flatMap {
            CGImageSourceCreateImageAtIndex($0, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
        }
    }

    private func url(_ key: String) -> URL { folder.appending(path: "\(key).jpg") }
}

extension PDFDocument {
    /// Page 1 as an image `pixelWidth` wide, or nil if there is no first page.
    fileprivate func cover(pixelWidth: CGFloat) -> CGImage? {
        guard let page = page(at: 0) else { return nil }
        let bounds = page.bounds(for: .cropBox)
        guard bounds.width > 0 else { return nil }
        let size = CGSize(width: pixelWidth, height: (pixelWidth * bounds.height / bounds.width).rounded())
        let image = page.thumbnail(of: size, for: .cropBox)
        #if canImport(UIKit)
            return image.cgImage
        #else
            return image.cgImage(forProposedRect: nil, context: nil, hints: nil)
        #endif
    }
}
