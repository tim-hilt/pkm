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
    /// The Drawings (as name and date, like the keys of `memory`) found to be no
    /// valid Drawing; they are not read again until they change.
    private var unreadable: Set<String> = []
    /// Made when first needed; stays nil without a GPU.
    private var renderer: ThumbnailRenderer?

    init(store: DrawingStore) {
        self.store = store
        memory.totalCostLimit = 48 << 20
    }

    /// What the Gallery shows for a Drawing.
    enum Thumbnail {
        /// The picture of the Drawing.
        case image(CGImage)
        /// The file is no valid Drawing (malformed): the Gallery shows an error.
        /// Nothing is ever written to such a file.
        case unreadable
        /// No picture for another reason (the Drawing is gone, no GPU).
        case unavailable

        var image: CGImage? {
            if case .image(let image) = self { image } else { nil }
        }

        var isUnreadable: Bool {
            if case .unreadable = self { true } else { false }
        }
    }

    /// Thumbnails drawn so far (not counting those from caches).
    var rendered: Int { renderer?.rendered ?? 0 }

    /// The thumbnail of the Drawing as listed in `entry`. Cheap when called again.
    func thumbnail(for entry: DrawingStore.Entry) async -> Thumbnail {
        let key = "\(entry.name)\u{0}\(entry.modified.timeIntervalSinceReferenceDate)"
        if let image = memory.object(forKey: key as NSString) { return .image(image) }
        if unreadable.contains(key) { return .unreadable }
        let image: CGImage
        if let cached = await Self.decodeCachedThumbnail(of: entry, in: store) {
            image = cached
        } else {
            switch await Self.prepareScene(of: entry, in: store) {
            case .gone:
                return .unavailable
            case .unreadable:
                unreadable.insert(key)
                return .unreadable
            case .scene(let scene):
                guard let rendered = await render(scene, for: entry) else { return .unavailable }
                image = rendered
            }
        }
        memory.setObject(image, forKey: key as NSString, cost: image.bytesPerRow * image.height)
        return .image(image)
    }

    private func render(_ scene: ThumbnailScene, for entry: DrawingStore.Entry) async -> CGImage? {
        guard !Task.isCancelled else { return nil }
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

    private enum Prepared: Sendable {
        case scene(ThumbnailScene)
        case unreadable
        case gone
    }

    private nonisolated static func prepareScene(of entry: DrawingStore.Entry, in store: DrawingStore) async -> Prepared
    {
        guard let bytes = try? store.open(entry.name) else { return .gone }
        guard let scene = ThumbnailScene(drawing: bytes) else { return .unreadable }
        return .scene(scene)
    }

    /// Failing to cache costs only the drawing of the thumbnail again.
    private nonisolated static func cache(
        _ image: RenderedImage, for entry: DrawingStore.Entry, in store: DrawingStore
    ) async {
        try? store.saveThumbnail(image.pngData(), for: entry)
    }
}
