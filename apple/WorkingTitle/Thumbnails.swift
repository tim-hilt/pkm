import CoreGraphics
import Foundation
import ImageIO

/// The thumbnails of the Gallery's Drawings, as pictures to show.
///
/// A thumbnail comes from the first of these that has a current one: memory,
/// the cache of the `DrawingStore` (outside the Drawing's file, see there),
/// and last the renderer, which then also fills the cache. A Drawing that
/// changed has a new `DrawingStore.Entry.modified`, so its old thumbnail is
/// simply not current any more.
///
/// For a smooth Gallery the main thread only draws: reading, decoding,
/// loading the Drawing and encoding all happen elsewhere, and the one GPU step
/// is short and one at a time.
@MainActor
final class Thumbnails {
    private let store: DrawingStore
    /// Pictures already made, dropped by the system when memory is short.
    private let memory = NSCache<NSString, CGImage>()
    /// Made when first needed; stays nil without a GPU.
    private var renderer: ThumbnailRenderer?

    init(store: DrawingStore) {
        self.store = store
        memory.totalCostLimit = 48 << 20
    }

    /// Thumbnails drawn so far (not counting those from caches).
    var rendered: Int { renderer?.rendered ?? 0 }

    /// The thumbnail of the Drawing as listed in `entry`; nil if it has none
    /// (the file is no Drawing, or is gone). Cheap when called again.
    func image(for entry: DrawingStore.Entry) async -> CGImage? {
        let key = "\(entry.name)\u{0}\(entry.modified.timeIntervalSinceReferenceDate)" as NSString
        if let image = memory.object(forKey: key) { return image }
        var image = await cachedImage(for: entry)
        if image == nil { image = await render(entry) }
        if let image {
            memory.setObject(image, forKey: key, cost: image.bytesPerRow * image.height)
        }
        return image
    }

    private func cachedImage(for entry: DrawingStore.Entry) async -> CGImage? {
        await Self.decodeCachedThumbnail(of: entry, in: store)
    }

    private func render(_ entry: DrawingStore.Entry) async -> CGImage? {
        guard let scene = await Self.prepareScene(of: entry, in: store), !Task.isCancelled else { return nil }
        if renderer == nil { renderer = ThumbnailRenderer() }
        guard let rendered = renderer?.render(scene) else { return nil }
        await Self.cache(rendered, for: entry, in: store)
        return rendered.cgImage()
    }

    // The steps that stay off the main thread.

    private nonisolated static func decodeCachedThumbnail(
        of entry: DrawingStore.Entry, in store: DrawingStore
    ) async -> CGImage? {
        guard let png = store.thumbnail(for: entry), let source = CGImageSourceCreateWithData(png as CFData, nil)
        else { return nil }
        // Decoded here, not when the picture is first drawn.
        return CGImageSourceCreateImageAtIndex(
            source, 0, [kCGImageSourceShouldCacheImmediately: true] as CFDictionary)
    }

    private nonisolated static func prepareScene(
        of entry: DrawingStore.Entry, in store: DrawingStore
    ) async -> ThumbnailScene? {
        guard let bytes = try? store.open(entry.name) else { return nil }
        return ThumbnailScene(drawing: bytes)
    }

    /// Failing to cache costs only the drawing of the thumbnail again.
    private nonisolated static func cache(
        _ image: RenderedImage, for entry: DrawingStore.Entry, in store: DrawingStore
    ) async {
        try? store.saveThumbnail(image.pngData(), for: entry)
    }
}
